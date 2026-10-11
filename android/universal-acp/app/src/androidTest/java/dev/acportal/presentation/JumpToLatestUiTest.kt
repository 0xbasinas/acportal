package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

class JumpToLatestUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(90)
    @Test fun darkReadingEarlierTextOffersJumpToLatest()=check("dark")
    @Test fun lightReadingEarlierTextOffersJumpToLatest()=check("light")
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java)
            .putExtra("page","conversation").putExtra("mode",mode)).use {scenario->
            compose.waitForIdle()
            val list=compose.onNodeWithTag("session-timeline")
            // Reading earlier text: follow-latest turns off and the button appears.
            list.performTouchInput {swipeDown()}
            compose.waitUntil(5000) {compose.onAllNodesWithTag("jump-to-latest").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Jump to latest message").assertIsDisplayed()
            compose.onNodeWithTag("jump-to-latest").performClick()
            // Back at the newest text: follow-latest resumes and the button goes away.
            compose.waitUntil(5000) {compose.onAllNodesWithTag("jump-to-latest").fetchSemanticsNodes().isEmpty()}
            assertTrue(compose.onAllNodesWithTag("jump-to-latest").fetchSemanticsNodes().isEmpty())
            scenario.onActivity {it.finishAndRemoveTask()}
        }
    }
}
