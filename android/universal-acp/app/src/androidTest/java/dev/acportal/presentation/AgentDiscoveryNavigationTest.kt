package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class AgentDiscoveryNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun settingsDiscoversRegistryAndPreselectsAgent() {
        val host=runBlocking {(compose.activity.application as PortalApplication).repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired mock host",host!=null)
        compose.onNodeWithContentDescription("Settings tab").performClick()
        compose.onNodeWithText("Agents").performClick()
        compose.onNodeWithText(host!!.label).performClick()
        compose.waitUntil(20_000) {compose.onAllNodesWithContentDescription("Inspect Mock ACP").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithContentDescription("Inspect Mock ACP").performClick()
        compose.waitUntil(20_000) {compose.onAllNodesWithText("New session").filter(isEnabled()).fetchSemanticsNodes().isNotEmpty() && runCatching {compose.onNodeWithText("Availability").assertIsDisplayed()}.isSuccess}
        compose.onNodeWithText("Availability").assertIsDisplayed()
        compose.onNodeWithText("New session").assertIsEnabled()
        compose.waitForIdle()
        android.os.SystemClock.sleep(250)
        val instrumentation=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val screenshot=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"registry-agent.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
        screenshot.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-registry-agent-details.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
        compose.onNodeWithText("New session").performClick()
        compose.onNodeWithTag("new-session-choices").performScrollToNode(hasTestTag("agent-choice:mock"))
        compose.onNode(isSelected() and hasAnySibling(hasText("Mock ACP")),useUnmergedTree=true).assertExists()
        compose.onNodeWithText("Start session").assertIsEnabled()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.waitUntil(20_000) {compose.onAllNodesWithText("New session").filter(isEnabled()).fetchSemanticsNodes().isNotEmpty() && runCatching {compose.onNodeWithText("Availability").assertIsDisplayed()}.isSuccess}
        compose.onNodeWithText("Availability").assertIsDisplayed()
    }
}
