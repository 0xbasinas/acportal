package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Run after the host used by HostLifecycleTest restarts, keeping the app's data. */
class RestartLifecycleTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    private fun awaitText(text:String) {compose.waitUntil(20_000) {compose.onAllNodesWithText(text,substring=true).fetchSemanticsNodes().isNotEmpty()}}
    @Test fun interruptedHostSessionResumesOnlyAfterUserChoice() {
        val id=InstrumentationRegistry.getArguments().getString("interruptedSessionId")
        assumeTrue("Requires a paired restarted mock host and its interruptedSessionId",!id.isNullOrBlank())
        compose.onNodeWithText("Sessions").performClick()
        awaitText("Interrupted by host restart")
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasTestTag("session-$id"))
        compose.onNodeWithTag("session-$id").performClick()
        awaitText("Session interrupted")
        compose.onNodeWithText("Resume session").performClick()
        awaitText("Connected")
        compose.onNodeWithText("Message, @files").performTextInput("Continue after host restart")
        compose.onNodeWithContentDescription("Send message").performClick()
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        awaitText("Permission needed")
        compose.onNodeWithText("Allow once").performClick()
        awaitText("Mock lifecycle complete. No files were changed.")
        compose.onNodeWithText("Mock lifecycle complete. No files were changed.\n").assertIsDisplayed()
    }
}
