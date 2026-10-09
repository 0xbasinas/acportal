package dev.acportal.presentation

import android.content.ContextWrapper
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.data.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import okhttp3.mockwebserver.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger

/** Real navigation, controlled HTTP and owned stores; never edits production hosts. */
class AgentPageStateNavigationTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(90)
    @Test fun darkLoadingFailureDismissalAndExplicitReloadReachEmptyAgents()=check("dark")
    @Test fun lightLoadingFailureDismissalAndExplicitReloadReachEmptyAgents()=check("light")
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val root=File(context.cacheDir,"agent-page-${UUID.randomUUID()}").apply {mkdirs()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={File(root,"settings.preferences_pb")}))
        val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir()=root})
        val client=HostApi.client();val server=MockWebServer();val store=ViewModelStore()
        val hold=AtomicBoolean(false);val success=AtomicBoolean(false);val catalog=AtomicBoolean(false);val requests=AtomicInteger();val mutations=AtomicInteger()
        val entered=CountDownLatch(1);val release=CountDownLatch(1)
        server.dispatcher=object:Dispatcher() {
            override fun dispatch(request:RecordedRequest):MockResponse {
                requests.incrementAndGet()
                if(request.method!="GET")mutations.incrementAndGet()
                if(hold.get()) {entered.countDown();check(release.await(30,TimeUnit.SECONDS)) {"Fixture response was never released"}}
                if(!success.get())return MockResponse().setResponseCode(503).setBody("{}")
                val body=when {
                    request.path=="/v1/status"->"""{"hostId":"fixture-host","name":"Fixture","version":"fixture"}"""
                    request.path=="/v1/agents" && catalog.get()->(0..29).joinToString(",","[","]") {"""{"id":"fixture-$it","name":"Fixture agent $it","installed":true,"status":"available"}"""}
                    else->"[]"
                }
                return MockResponse().setBody(body)
            }
        }
        server.start()
        try {
            runBlocking {database.portal().saveHost(HostProfile("fixture-host","Isolated computer",server.url("/").toString(),vault.save("fixture-only"),"fixture-device",Long.MAX_VALUE))}
            val repository=PortalRepository(database.portal(),HostApi(client,vault),vault,settings,scope)
            val vm=PortalViewModel(repository);store.put("fixture",vm)
            compose.setContent {PortalTheme(mode) {PortalNavigation(vm)}}
            compose.waitUntil(10_000) {requests.get()>0 && !vm.ui.value.loading}
            hold.set(true)
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("Agents").performScrollTo().performClick()
            compose.onNodeWithText("Isolated computer").performClick()
            compose.waitUntil(10_000) {entered.count==0L}
            compose.onNodeWithText("Loading agents").assertIsDisplayed()
            release.countDown();hold.set(false)
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
            compose.onNodeWithText("Agents unavailable").assertIsDisplayed()
            compose.onNodeWithText("Loading agents").assertDoesNotExist()
            val before=requests.get()
            compose.onNodeWithContentDescription("Dismiss error").performClick()
            compose.onNodeWithText("Reload agents").assertIsDisplayed().assertIsEnabled()
            compose.runOnIdle {assertEquals(before,requests.get())}
            success.set(true)
            compose.onNodeWithText("Reload agents").performTouchInput {click()}
            compose.waitUntil(10_000) {compose.onAllNodesWithText("No agents configured").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            compose.onNodeWithText("No agents configured").assertIsDisplayed()
            catalog.set(true)
            compose.onNodeWithContentDescription("Refresh agents").performClick()
            compose.waitUntil(10_000) {vm.ui.value.details?.agents?.size==30 && !vm.ui.value.loading}
            compose.onNodeWithTag("agent-list").performScrollToNode(hasText("Fixture agent 18"))
            val anchor=compose.onNodeWithText("Fixture agent 18").getUnclippedBoundsInRoot()
            compose.onNodeWithText("Fixture agent 18").performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithText("Agent ID").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            compose.onNodeWithContentDescription("Back").performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithTag("agent-list").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            // Verify returned scroll before any scroll action can repair it.
            compose.onNodeWithText("Fixture agent 18").assertIsDisplayed()
            val returned=compose.onNodeWithText("Fixture agent 18").getUnclippedBoundsInRoot()
            assertTrue("Agent list anchor moved after details/Back",kotlin.math.abs((returned.top-anchor.top).value)<3f)
            assertEquals(0,mutations.get())
            compose.onNodeWithContentDescription("Back").performClick()
            compose.onNodeWithText("Isolated computer").assertExists()
        } finally {
            release.countDown();compose.runOnIdle {store.clear()}
            runBlocking {scope.coroutineContext[Job]!!.cancelAndJoin()}
            database.close();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()
            assertTrue(root.deleteRecursively())
        }
    }
}
