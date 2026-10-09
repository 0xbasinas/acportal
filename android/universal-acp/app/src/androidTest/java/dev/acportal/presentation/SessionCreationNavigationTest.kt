package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.data.*
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.encodeToString
import okhttp3.*
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger

class SessionCreationNavigationTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    @Test fun sessionsPlusPreservesSearchAndFailedStartNeverRetries() {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val prefs=File(context.cacheDir,"creation-${UUID.randomUUID()}.preferences_pb")
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={prefs}))
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val vault=CredentialVault(context);val alias=vault.save("isolated-creation-fixture")
        val store=ViewModelStore();val mutations=AtomicInteger()
        val client=HostApi.client().newBuilder().addInterceptor {chain->
            val request=chain.request();val mutation=request.method!="GET"
            val body=if(mutation) {mutations.incrementAndGet();"{}"} else when(request.url.encodedPath) {
                "/v1/status"->WireJson.encodeToString(HostStatus("fixture","Workstation","fixture"))
                "/v1/agents"->WireJson.encodeToString(listOf(AgentInfo("goose","Goose",installed=true)))
                "/v1/workspaces"->WireJson.encodeToString(listOf(Workspace("/work","Project")))
                else->"[]"
            }
            Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(if(mutation)503 else 200).message("Fixture").body(body.toResponseBody("application/json".toMediaType())).build()
        }.build()
        try {
            runBlocking {database.portal().saveHost(HostProfile("fixture","Workstation","https://fixture.invalid",alias,"device",0));settings.settings.first()}
            val repository=PortalRepository(database.portal(),HostApi(client,vault),vault,settings,scope)
            val vm=PortalViewModel(repository);store.put("creation",vm)
            compose.setContent {PortalTheme {PortalNavigation(vm)}}
            compose.waitUntil(10_000) {compose.onAllNodesWithContentDescription("Sessions tab").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            compose.onNodeWithContentDescription("Sessions tab").performClick()
            compose.onNode(hasSetTextAction()).performTextReplacement("retained search")
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            compose.onNodeWithContentDescription("New session").performClick()
            compose.onNodeWithText("Choose a connection").assertIsDisplayed()
            compose.onNodeWithText("Workstation").performClick()
            compose.waitUntil(10_000) {compose.onAllNodesWithText("Start session").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            assertEquals(0,mutations.get())
            compose.onNodeWithText("Start session").performClick()
            compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
            assertEquals(1,mutations.get());assertTrue(runBlocking {repository.sessions.first().isEmpty()})
            compose.onNodeWithContentDescription("Dismiss error").performClick()
            compose.onNodeWithText("Start session").assertIsEnabled()
            compose.onNodeWithContentDescription("Back").performClick()
            compose.onNodeWithContentDescription("Back").performClick()
            compose.onNode(hasSetTextAction()).assertTextEquals("retained search")
            compose.runOnIdle {assertEquals(1,mutations.get())}
        } finally {
            compose.runOnIdle {store.clear()};scope.cancel();database.close();vault.remove(alias)
            client.dispatcher.executorService.shutdown();client.connectionPool.evictAll();prefs.delete()
        }
    }
}
