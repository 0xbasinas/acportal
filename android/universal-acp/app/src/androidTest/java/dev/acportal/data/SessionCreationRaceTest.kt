package dev.acportal.data

import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.presentation.PortalViewModel
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.HostProfile
import dev.acportal.storage.PortalDatabase
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.encodeToString
import org.junit.After
import org.junit.Assert.*
import org.junit.Before
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/**
 * Session creation and host reads against several loopback fixture hosts, some slow.
 * Uses an in-memory database and the app's own HostApi, so saved data is untouched.
 * Instrumented because the repository needs Room and the Android keystore (runbook §5).
 */
class SessionCreationRaceTest {
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    private val context=InstrumentationRegistry.getInstrumentation().targetContext
    private val app=context.applicationContext as PortalApplication
    private val vault=CredentialVault(context)
    private val aliases=mutableListOf<String>()
    private val hosts=mutableListOf<FixtureHost>()
    private val viewModels=androidx.lifecycle.ViewModelStore()
    private lateinit var database:PortalDatabase
    private lateinit var scope:CoroutineScope
    private lateinit var repository:PortalRepository

    @Before fun setUp() {
        database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        repository=PortalRepository(database.portal(),app.repository.api,vault,app.repository.settings,scope)
    }
    @After fun tearDown() {
        InstrumentationRegistry.getInstrumentation().runOnMainSync {viewModels.clear()}
        hosts.forEach {it.close()}
        aliases.forEach {runCatching {vault.remove(it)}}
        scope.cancel();database.close()
    }

    private fun token()=vault.save("fixture-token-${aliases.size}").also {aliases+=it}
    private fun host(id:String,onCreate:()->Unit={},onStatus:()->Unit={}):Pair<HostProfile,FixtureHost> {
        val fixture=FixtureHost {method,path,_->
            when {
                path=="/v1/status"->{onStatus();200 to WireJson.encodeToString(HostStatus(id,"Fixture $id","test"))}
                path=="/v1/agents"->200 to WireJson.encodeToString(listOf(AgentInfo("mock","Mock ACP",installed=true,status="available")))
                path=="/v1/workspaces"->200 to WireJson.encodeToString(listOf(Workspace("/workspace/$id","Folder $id")))
                method=="GET" && path=="/v1/sessions"->200 to "[]"
                method=="POST" && path=="/v1/sessions"->{onCreate();200 to WireJson.encodeToString(SessionInfo("session-$id","mock","acp-$id","/workspace/$id"))}
                else->404 to "{}"
            }
        }.also {hosts+=it}
        val profile=HostProfile(id,"Fixture $id",fixture.address,token(),"device-$id",Long.MAX_VALUE)
        runBlocking {database.portal().saveHost(profile)}
        return profile to fixture
    }

    @Test fun creationsOnTwoHostsFinishingOutOfOrderAreSavedUnderTheirOwnHosts()=runBlocking {
        val release=CountDownLatch(1)
        val (slow,_)=host("slow",onCreate={release.await(20,TimeUnit.SECONDS)})
        val (fast,_)=host("fast")
        val first=async(Dispatchers.IO) {repository.create(slow.id,"mock","/workspace/slow")}
        val second=async(Dispatchers.IO) {repository.create(fast.id,"mock","/workspace/fast")}
        val fastSession=second.await()
        assertFalse("the slow creation must still be pending",first.isCompleted)
        release.countDown()
        val slowSession=first.await()
        assertEquals("fast" to "session-fast",fastSession.hostId to fastSession.id)
        assertEquals("slow" to "session-slow",slowSession.hostId to slowSession.id)
        val stored=database.portal().sessions().first().associate {it.id to it.hostId}
        assertEquals(mapOf("session-slow" to "slow","session-fast" to "fast"),stored)
        assertEquals(setOf("slow","fast"),database.portal().recent("slow").map {it.hostId}.toSet()+database.portal().recent("fast").map {it.hostId})
    }

    @Test fun aSlowHostReadDoesNotRestoreACredentialReplacedMeanwhile()=runBlocking {
        val entered=CountDownLatch(1);val release=CountDownLatch(1)
        val (profile,_)=host("repaired",onStatus={entered.countDown();release.await(20,TimeUnit.SECONDS)})
        val read=async(Dispatchers.IO) {repository.details(profile.id)}
        assertTrue(entered.await(10,TimeUnit.SECONDS))
        // A re-pair during the slow read replaces the credential (repository.pair does this).
        val replacement=token()
        database.portal().saveHost(profile.copy(credentialAlias=replacement))
        release.countDown()
        read.await()
        val saved=database.portal().host(profile.id)!!
        assertEquals("the status write must keep the new credential",replacement,saved.credentialAlias)
        assertTrue(saved.online)
        assertEquals(1,saved.agentCount)
    }

    @Test fun aFailedSlowReadMarksOfflineWithoutRestoringTheOldCredential()=runBlocking {
        val entered=CountDownLatch(1);val release=CountDownLatch(1)
        lateinit var fixture:FixtureHost
        val (profile,created)=host("failing",onStatus={entered.countDown();release.await(20,TimeUnit.SECONDS);fixture.close()})
        fixture=created
        val read=async(Dispatchers.IO) {runCatching {repository.details(profile.id)}}
        assertTrue(entered.await(10,TimeUnit.SECONDS))
        val replacement=token()
        database.portal().saveHost(profile.copy(credentialAlias=replacement,online=true))
        release.countDown()
        assertTrue(read.await().isFailure)
        val saved=database.portal().host(profile.id)!!
        assertEquals(replacement,saved.credentialAlias)
        assertFalse(saved.online)
    }

    @Test fun viewModelIgnoresARepeatedCreateAndStaysLoadingUntilTheSlowOneFinishes() {
        val entered=CountDownLatch(1);val release=CountDownLatch(1)
        val (slow,fixture)=host("vm",onCreate={entered.countDown();release.await(20,TimeUnit.SECONDS)})
        lateinit var vm:PortalViewModel
        val done=CountDownLatch(1)
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        instrumentation.runOnMainSync {
            vm=PortalViewModel(repository)
            viewModels.put("fixture",vm)
            vm.create(slow.id,"mock","/workspace/vm") {done.countDown()}
            vm.create(slow.id,"mock","/workspace/vm") {fail("a repeated request must not start a second session")}
        }
        assertTrue(entered.await(10,TimeUnit.SECONDS))
        // A short operation that finishes first must not clear the loading state.
        instrumentation.runOnMainSync {vm.refresh()}
        runBlocking {
            // Status reads: the view model's initial refresh, the creation's feature check, this refresh.
            withTimeout(10_000) {while(fixture.count("GET /v1/status")<3)delay(20)}
            delay(300)
        }
        instrumentation.runOnMainSync {
            assertTrue("loading must stay on while creation runs",vm.ui.value.loading)
            assertTrue(vm.creationInFlight(slow.id,"mock","/workspace/vm"))
        }
        release.countDown()
        assertTrue(done.await(10,TimeUnit.SECONDS))
        instrumentation.waitForIdleSync()
        runBlocking {withTimeout(5_000) {while(vm.ui.value.loading)delay(20)}}
        assertEquals(1,fixture.count("POST /v1/sessions"))
        instrumentation.runOnMainSync {assertFalse(vm.creationInFlight(slow.id,"mock","/workspace/vm"))}
    }
}
