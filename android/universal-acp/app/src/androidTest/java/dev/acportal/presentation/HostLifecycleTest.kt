package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import org.junit.Assert.assertEquals
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Run with the pairingCode runner argument and an acpd mock host on port 8767. */
class HostLifecycleTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    private fun awaitText(text:String) {
        try {compose.waitUntil(20_000) {compose.onAllNodesWithText(text,substring=true).fetchSemanticsNodes().isNotEmpty()}}
        catch(error:Throwable) {compose.onRoot().printToLog("AcportalFailure");throw error}
    }
    @Test fun pairedHostStreamsPermissionToolsAndDiff() {
        val code=InstrumentationRegistry.getArguments().getString("pairingCode")
        assumeTrue("Requires a running mock host and a fresh pairingCode",!code.isNullOrBlank())
        compose.onNodeWithText("Add connection").performClick()
        compose.onNodeWithText("Connection name").performTextInput("Mock development host")
        compose.onNodeWithText("Address").performTextInput("http://10.0.2.2:8767")
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Pairing code"))
        compose.onNodeWithText("Pairing code").performTextInput(code!!)
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Pair connection"))
        compose.onNodeWithText("Pair connection").performClick()
        awaitText("Connections")
        awaitText("Mock development host")
        compose.onNodeWithText("Mock development host").performClick()
        awaitText("New session")
        compose.onNodeWithContentDescription("Inspect Mock ACP").performClick()
        awaitText("Agent ID")
        compose.onNodeWithText("Availability").assertIsDisplayed()
        compose.onNodeWithText("Start a session to see the capabilities, models and sign-in methods reported by this agent.").assertIsDisplayed()
        capture("registry-agent-details")
        compose.onNodeWithText("New session").performClick()
        awaitText("Mock ACP")
        compose.onNodeWithText("Mock ACP").assertIsDisplayed()
        val expectAccess=InstrumentationRegistry.getArguments().getString("expectWorkspaceAccess")=="true"
        if(expectAccess) {
            compose.onNodeWithText("Workspace access").performClick()
            awaitText("Save access")
            compose.onNodeWithContentDescription("Write files").performClick()
            capture("workspace-access")
            compose.onNodeWithText("Save access").performClick()
            awaitText("Start session")
        }
        val mcpCommand=InstrumentationRegistry.getArguments().getString("mcpCommand")
        if(!mcpCommand.isNullOrBlank()) {
            compose.onNodeWithText("MCP servers").performClick()
            awaitText("+  Add server")
            compose.onNodeWithText("+  Add server").performClick()
            compose.onNodeWithText("Server name").performTextInput("Workspace tools")
            compose.onNodeWithText("Executable on host").performTextInput(mcpCommand)
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            compose.onNodeWithText("Save server").performClick()
            awaitText("+  Add server")
            compose.onNodeWithText("Workspace tools").assertIsDisplayed()
            capture("mcp-servers")
            compose.onNodeWithContentDescription("Back").performClick()
            awaitText("Start session")
        }
        capture("new-session")
        compose.onNodeWithText("Start session").performClick()
        awaitText("Connected")
        if(InstrumentationRegistry.getArguments().getString("expectAuthentication")=="true") {
            awaitText("Sign in to mock")
            compose.onNodeWithText("Message, @files").performTextInput("Draft kept while signing in")
            compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            compose.onNodeWithText("Mock sign-in").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("Sign in to mock").fetchSemanticsNodes().isEmpty()}
            compose.onNodeWithContentDescription("Send message").assertIsEnabled()
            compose.onNodeWithText("Draft kept while signing in").performTextClearance()
        }
        if(!mcpCommand.isNullOrBlank())runBlocking {
            val repository=(compose.activity.application as PortalApplication).repository
            val host=repository.hosts.first().single()
            val stored=repository.sessions.first().first {it.hostId==host.id}
            val session=repository.api.session(host,stored.id)
            assertEquals("Workspace tools",session.setup["_meta"].objectValue()["mockMcpServers"].arrayValue().single().objectValue()["name"].text())
            if(expectAccess)assertEquals(WorkspaceAccess(true,false,true),session.workspaceAccess)
        }
        compose.onNodeWithText("Message, @files").performTextInput("Inspect the mock workspace")
        compose.onNodeWithContentDescription("Send message").performClick()
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        if(InstrumentationRegistry.getArguments().getString("expectTerminal")=="true") {
            awaitText("Host permission needed")
            compose.onNodeWithText("Run once").performClick()
        }
        awaitText("Permission needed")
        compose.onNodeWithText("Propose a mock file change").assertIsDisplayed()
        capture("permission")
        compose.onNodeWithText("Allow once").performClick()
        awaitText("Mock lifecycle complete. No files were changed.")
        compose.onNodeWithText("Mock lifecycle complete. No files were changed.\n").assertIsDisplayed()
        compose.onNodeWithText("Permission needed").assertDoesNotExist()
        capture("conversation-ready")
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Proposed example.rs change"))
        compose.onNode(hasContentDescription("Expand tool") and hasAnyAncestor(hasTestTag("tool-card") and hasAnyDescendant(hasText("Proposed example.rs change")))).performClick()
        compose.onNodeWithText("let expired = false;",substring=true).assertExists()
        compose.onNodeWithText("View changes").performClick()
        awaitText("Changes")
        compose.onNodeWithText("let expired = false;").assertExists()
        capture("changes-live")
        compose.onNodeWithContentDescription("Back").performClick()
        if(InstrumentationRegistry.getArguments().getString("expectTerminal")=="true") {
            compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Run ACP terminal fixture"))
            compose.onNode(hasContentDescription("Expand tool") and hasAnyAncestor(hasTestTag("tool-card") and hasAnyDescendant(hasText("Run ACP terminal fixture")))).performClick()
            compose.onNodeWithText("ACP terminal output verified.\n").assertIsDisplayed()
            compose.onNodeWithText("Exit code 0").assertIsDisplayed()
        }
        capture("session")
        compose.onNodeWithContentDescription("Session menu").performClick()
        compose.onNodeWithText("Agent details").performClick()
        compose.onNodeWithText("Session history").assertIsDisplayed()
        capture("agent-details")
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithContentDescription("Session menu").performClick()
        compose.onNodeWithText("Session options").performClick()
        compose.onNodeWithText("This agent has not supplied session options.").assertIsDisplayed()
        capture("session-options")
        compose.onNodeWithText("Done").performClick()
        compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Session menu").performClick()
        compose.onNodeWithText("Connection details").performClick()
        compose.onNodeWithText("STDIO").assertIsDisplayed()
        capture("connection-details")
        compose.onNodeWithText("Disconnect").performClick()
        awaitText("Disconnected")
        compose.onNodeWithText("Disconnect").assertIsNotEnabled()
        compose.onNodeWithText("Reconnect").performClick()
        awaitText("• Connected")
        compose.onNodeWithText("View connection logs").performClick()
        awaitText("WebSocket connected")
        capture("connection-logs")
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("Mock lifecycle complete. No files were changed.\n"))
        compose.onNodeWithText("Mock lifecycle complete. No files were changed.\n").assertExists()
        compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
        if(expectAccess) {
            compose.onNodeWithContentDescription("Session menu").performClick()
            compose.onNodeWithText("Workspace access").performClick()
            awaitText("Save for next session")
            compose.onNodeWithContentDescription("Write files").assertIsOff()
            compose.onNodeWithContentDescription("Read files").performClick()
            compose.onNodeWithContentDescription("Terminal").performClick()
            capture("workspace-access-thread")
            compose.onNodeWithText("Save for next session").performClick()
            awaitText("Message, @files")
            runBlocking {
                val repository=(compose.activity.application as PortalApplication).repository
                val host=repository.hosts.first().single()
                val stored=repository.sessions.first().first {it.hostId==host.id}
                val current=repository.api.session(host,stored.id)
                assertEquals(WorkspaceAccess(true,false,true),current.workspaceAccess)
                val next=repository.create(host.id,current.agentId,current.workspace,current.acpSessionId)
                assertEquals(WorkspaceAccess(false,false,false),repository.api.session(host,next.id).workspaceAccess)
                assertEquals(WorkspaceAccess(true,false,true),repository.api.session(host,stored.id).workspaceAccess)
                repository.delete(next)
            }
        }
        compose.onNodeWithContentDescription("Session menu").performClick()
        compose.onNodeWithText("Export transcript").assertExists()
        compose.onNodeWithText("Remove local copy").assertExists()
        capture("session-menu")
        if(InstrumentationRegistry.getArguments().getString("expectLocalCopy")=="true") {
            compose.onNodeWithText("Remove local copy").performClick()
            compose.onNodeWithText("Cancel").performClick()
            compose.onNodeWithText("Message, @files").performTextInput("Draft removed with local copy")
            androidx.test.espresso.Espresso.closeSoftKeyboard()
            compose.onNodeWithContentDescription("Session menu").performClick()
            compose.onNodeWithText("Remove local copy").performClick()
            compose.onNodeWithText("Remove",useUnmergedTree=true).performClick()
            awaitText("Earlier messages were removed from this device.")
            compose.onNodeWithText("Mock lifecycle complete. No files were changed.\n").assertDoesNotExist()
            compose.onNodeWithText("Draft removed with local copy").assertDoesNotExist()
            capture("local-copy-removed")
        } else {
            compose.onNodeWithText("Session options").performClick()
            compose.onNodeWithText("Done").performClick()
        }
    }
    private fun capture(name:String) {
        compose.waitForIdle()
        // Allow the emulator compositor to present the frame before the platform screenshot.
        android.os.SystemClock.sleep(250)
        val image=InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"session.png")
        file.parentFile!!.mkdirs();file.outputStream().use {image.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};image.recycle()
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
