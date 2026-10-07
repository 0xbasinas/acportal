package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.data.PortalRepository
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID

class PairingHostNavigationTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun invalidCodeThenSuccessfulPairingStoresAnAuthenticatedOneUseCredential()=checkPairing(true)
    @Test fun expiredCodeLeavesTheConnectionAbsent()=checkPairing(false)

    private class Fixture {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val file=File(context.cacheDir,"pairing-${UUID.randomUUID()}.preferences_pb")
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={file}))
        val vault=CredentialVault(context)
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val repository=PortalRepository(database.portal(),actual.api,vault,settings,scope)
        val store=ViewModelStore()
        fun close() {
            store.clear();runBlocking {scope.coroutineContext[Job]!!.cancelAndJoin()}
            database.close();file.delete();File(file.path+".tmp").delete()
        }
    }
    private fun code():String {
        val value=InstrumentationRegistry.getArguments().getString("pairingCode")
        assumeTrue("Requires a task-host one-use pairing code",value!=null)
        return value!!
    }
    private fun checkPairing(success:Boolean) {
        val code=code()
        if(!success)assumeTrue("Requires an operator-expired fixture code",InstrumentationRegistry.getArguments().getString("expiredFixture")=="true")
        val fixture=Fixture()
        val productionHosts=runBlocking {fixture.actual.hosts.first()}
        var created:HostProfile?=null
        try {
            val vm=PortalViewModel(fixture.repository);fixture.store.put("pairing",vm)
            compose.setContent {PortalTheme {PortalNavigation(vm)}}
            compose.waitUntil(10_000) {!vm.ui.value.loading && compose.onAllNodesWithText("Add connection").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Add connection").performClick()
            scroll("Connection name");compose.onNodeWithText("Connection name").performTextInput("Pairing fixture")
            scroll("Address");compose.onNodeWithText("Address").performTextInput("http://10.0.2.2:8767")
            if(success) {
                scroll("Pairing code");compose.onNodeWithText("Pairing code").performTextInput("INVALID-CODE")
                submit()
                compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
                assertTrue(vm.ui.value.error!!.contains("Pairing code was rejected"))
                assertTrue(runBlocking {fixture.repository.hosts.first().isEmpty()})
                compose.onNodeWithContentDescription("Dismiss error").performClick()
            }
            scroll("Pairing code");compose.onNodeWithText("Pairing code").performTextReplacement(code)
            submit()
            if(!success) {
                compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
                assertTrue(vm.ui.value.error!!.contains("Pairing code was rejected"))
                assertTrue(runBlocking {fixture.repository.hosts.first().isEmpty()})
                compose.onNodeWithText("Pairing code was rejected.",substring=true).assertIsDisplayed()
            } else {
                compose.waitUntil(20_000) {!vm.ui.value.loading && compose.onAllNodesWithText("Pairing fixture").fetchSemanticsNodes().isNotEmpty()}
                created=runBlocking {fixture.repository.hosts.first().single()}
                assertEquals(created!!.id,runBlocking {fixture.actual.api.status(created!!).hostId})
                val token=fixture.vault.read(created!!.credentialAlias)
                val bytes=File(fixture.context.noBackupFilesDir,"credentials/${created!!.credentialAlias}").readBytes()
                assertFalse(bytes.toString(Charsets.ISO_8859_1).contains(token))
                val before=created!!
                val rejected=runBlocking {runCatching {fixture.repository.pair(before.address,code,"Pairing fixture")}.exceptionOrNull()}
                assertTrue(rejected?.message?.contains("Pairing code was rejected")==true)
                assertEquals(before,runBlocking {fixture.repository.hosts.first().single()})
                assertEquals(before.id,runBlocking {fixture.actual.api.status(before).hostId})
            }
            assertTrue(runBlocking {fixture.actual.hosts.first()}==productionHosts)
        } finally {
            val paired=created ?: runBlocking {fixture.repository.hosts.first().firstOrNull()}
            try {paired?.let {runBlocking {fixture.repository.forget(it.id)}}} finally {fixture.close()}
        }
    }
    @Test fun renewalReplacesOnlyTheFixtureAliasAndPreservesExistingPhoneCredentials() {
        val code=code();val fixture=Fixture()
        val original=runBlocking {fixture.actual.hosts.first().firstOrNull()}
        assumeTrue("Requires the existing paired task host",original!=null)
        val copiedAlias=fixture.vault.save(fixture.actual.api.token(original!!))
        var renewed:HostProfile?=null
        try {
            runBlocking {fixture.database.portal().saveHost(original.copy(credentialAlias=copiedAlias))}
            renewed=runBlocking {fixture.repository.pair(original.address,code,"Renewal fixture")}
            assertNotEquals(copiedAlias,renewed!!.credentialAlias)
            assertFalse(File(fixture.context.noBackupFilesDir,"credentials/$copiedAlias").exists())
            assertEquals(original.id,runBlocking {fixture.actual.api.status(renewed!!).hostId})
            assertEquals(original.id,runBlocking {fixture.actual.api.status(original).hostId})
            assertEquals(original.credentialAlias,runBlocking {fixture.actual.hosts.first().single {it.id==original.id}.credentialAlias})
        } finally {
            try {renewed?.let {runBlocking {fixture.repository.forget(it.id)}}} finally {fixture.vault.remove(copiedAlias);fixture.close()}
        }
    }
    private fun scroll(label:String) {
        compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(label))
    }
    private fun submit() {
        androidx.test.espresso.Espresso.closeSoftKeyboard();scroll("Pair connection")
        compose.onNodeWithText("Pair connection").performClick()
    }
}
