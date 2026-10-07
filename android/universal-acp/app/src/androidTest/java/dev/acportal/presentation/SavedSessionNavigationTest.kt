package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.PortalDatabase
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.net.InetSocketAddress
import java.net.ServerSocket

class SavedSessionNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun hostFailureShowsSavedConversationAndExplicitRetryOpensLiveSession() {
        val repository=(compose.activity.application as PortalApplication).repository
        val host=runBlocking {repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        val original=host!!
        val previousUi=runBlocking {repository.uiState.first()}
        lateinit var vm:PortalViewModel
        compose.runOnIdle {vm=ViewModelProvider(compose.activity)[PortalViewModel::class.java]}
        compose.waitUntil(20_000) {!vm.ui.value.loading}
        val stored=runBlocking {repository.create(original.id,"mock",repository.api.workspaces(original).first().path)}
        val database=PortalDatabase.open(compose.activity)
        val unavailable=ServerSocket().apply {bind(InetSocketAddress("127.0.0.1",0))}
        val responder=Thread {
            try {while(!unavailable.isClosed)unavailable.accept().use {socket->
                socket.soTimeout=3000
                val reader=socket.getInputStream().bufferedReader()
                while(!reader.readLine().isNullOrEmpty()) {}
                socket.getOutputStream().write("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".toByteArray());socket.getOutputStream().flush()
            }} catch(_:Exception) {}
        }.apply {isDaemon=true;start()}
        try {
            runBlocking {
                val state=SessionState(items=listOf(TimelineItem.Text("cached","agent","Previously saved conversation")))
                database.portal().saveState(original.id,stored.id,WireJson.encodeToString(SessionState.serializer(),state),System.currentTimeMillis())
                repository.draft(original.id,stored.id,"Offline draft")
                database.portal().saveHost(original.copy(address="http://127.0.0.1:${unavailable.localPort}",online=false))
                repository.saveSessionFilters(false,false,"");repository.refreshHosts()
            }
            compose.onNodeWithContentDescription("Sessions tab").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithTag("session-${stored.id}").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithTag("session-${stored.id}").performClick()
            compose.waitUntil(20_000) {!vm.ui.value.loading && compose.onAllNodesWithText("Retry").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Saved conversation").assertIsDisplayed()
            compose.onNodeWithText("Previously saved conversation").assertIsDisplayed()
            compose.onNode(hasSetTextAction()).assertTextEquals("Offline draft").performTextReplacement("Edited offline draft")
            compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
            compose.waitUntil(10_000) {runBlocking {repository.sessions.first().single {it.id==stored.id}.draft=="Edited offline draft"}}
            compose.onNode(hasSetTextAction()).assertIsDisplayed()
            compose.onNodeWithContentDescription("Send message").assertIsDisplayed()
            capture("saved-conversation-keyboard")
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            compose.waitUntil(10_000) {compose.onAllNodesWithText("Connect to host").fetchSemanticsNodes().isNotEmpty()}
            capture()
            runBlocking {database.portal().saveHost(original)}
            compose.onNodeWithText("Connect to host").performClick()
            compose.waitUntil(20_000) {vm.ui.value.live?.takeIf {it.info.id==stored.id}?.let {it.connection.value==ConnectionState.Connected && !it.state.value.replaying}==true}
            compose.onNodeWithText("Saved conversation").assertDoesNotExist()
            assertEquals("Edited offline draft",runBlocking {repository.sessions.first().single {it.id==stored.id}.draft})
            assertFalse(vm.ui.value.live!!.state.value.processing)
        } finally {
            runBlocking {database.portal().saveHost(original);repository.delete(stored);repository.saveMainPage(previousUi.mainPage);repository.saveSessionFilters(previousUi.sessionsArchived,previousUi.sessionsActive,previousUi.sessionsQuery);repository.refreshHosts()}
            database.close();unavailable.close();responder.join(1000)
        }
    }
    private fun capture(name:String="saved-conversation") {
        compose.waitForIdle()
        val instrumentation=InstrumentationRegistry.getInstrumentation();val image=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"saved-conversation.png")
        file.parentFile!!.mkdirs();file.outputStream().use {image.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};image.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
