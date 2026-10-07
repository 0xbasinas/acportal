package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.PortalDatabase
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.net.InetSocketAddress
import java.net.ServerSocket

class AgentCacheRecoveryNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun failedDiscoveryShowsSavedAgentsAndRetryRestoresFreshLaunch() {
        val repository=(compose.activity.application as PortalApplication).repository
        val host=runBlocking {repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        val original=host!!
        lateinit var vm:PortalViewModel
        compose.runOnIdle {vm=ViewModelProvider(compose.activity)[PortalViewModel::class.java]}
        compose.waitUntil(20_000) {!vm.ui.value.loading}
        runBlocking {repository.details(original.id)}
        val saved=runBlocking {repository.agentCatalogs.first().single {it.hostId==original.id}}
        assertTrue(WireJson.decodeFromString<List<AgentInfo>>(saved.metadata).any {it.id=="mock"})
        val database=PortalDatabase.open(compose.activity)
        val unavailable=ServerSocket().apply {bind(InetSocketAddress("127.0.0.1",0))}
        val responder=Thread {
            try {
                while(!unavailable.isClosed)unavailable.accept().use {socket->
                    socket.soTimeout=3000
                    val reader=socket.getInputStream().bufferedReader()
                    while(!reader.readLine().isNullOrEmpty()) { /* discard request headers */ }
                    socket.getOutputStream().write("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".toByteArray())
                    socket.getOutputStream().flush()
                }
            } catch(_:Exception) { /* fixture shuts down by closing its listener */ }
        }.apply {isDaemon=true;start()}
        try {
            runBlocking {
                database.portal().saveHost(original.copy(address="http://127.0.0.1:${unavailable.localPort}",online=false))
                repository.refreshHosts()
            }
            compose.onNodeWithContentDescription("Settings tab").performClick()
            compose.onNodeWithText("Agents").performClick()
            compose.onNodeWithText(original.label).performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("Saved agent list",substring=true).fetchSemanticsNodes().isNotEmpty() && compose.onAllNodesWithText("Retry").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Last reported: Available").assertIsDisplayed()
            compose.onNodeWithContentDescription("Inspect Mock ACP").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("Last reported availability").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            compose.onNodeWithText("New session").assertIsNotEnabled()
            capture()
            runBlocking {database.portal().saveHost(original)}
            compose.onNodeWithText("Retry").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("New session").filter(isEnabled()).fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Saved agent list",substring=true).assertDoesNotExist()
            compose.onNodeWithText("Availability").assertIsDisplayed()
            val refreshed=runBlocking {repository.agentCatalogs.first().single {it.hostId==original.id}}
            assertTrue(refreshed.discoveredAt>=saved.discoveredAt)
        } finally {
            runBlocking {database.portal().saveHost(original);repository.refreshHosts()}
            database.close();unavailable.close();responder.join(1000)
        }
    }
    private fun capture() {
        compose.waitForIdle()
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val screenshot=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"saved-agent-details.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
        screenshot.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-saved-agent-details.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
