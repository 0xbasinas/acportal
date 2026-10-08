package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.unit.dp
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConversationUiRestorationTest {
    @get:Rule val compose=createEmptyComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(90)
    @Test fun darkRecreationRetainsUserScrollAndExpandedToolAndThought()=check("dark")
    @Test fun lightRecreationRetainsUserScrollAndExpandedToolAndThought()=check("light")
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java)
            .putExtra("page","conversation").putExtra("mode",mode)).use {scenario->
            val list=compose.onNodeWithTag("session-timeline")
            compose.waitForIdle()
            list.performTouchInput {swipeDown()}
            list.performScrollToNode(hasContentDescription("Expand tool: Restore anchor tool"))
            compose.onNodeWithContentDescription("Expand tool: Restore anchor tool").performClick()
            list.performScrollToNode(hasContentDescription("Show agent thoughts"))
            compose.onNodeWithContentDescription("Show agent thoughts").performClick()
            list.performScrollToNode(hasText("Restore expanded thought"))
            compose.onNodeWithText("Restore expanded thought").assertIsDisplayed()
            val before=compose.onNodeWithText("Restore expanded thought").getUnclippedBoundsInRoot()
            scenario.recreate()
            compose.waitForIdle()
            // Assert before any scroll action: a restored list must not jump to the latest turn.
            compose.onNodeWithText("Restore expanded thought").assertIsDisplayed()
            val after=compose.onNodeWithText("Restore expanded thought").getUnclippedBoundsInRoot()
            assertTrue("Recreation changed the visible anchor",kotlin.math.abs((after.top-before.top).value)<3.dp.value)
            compose.onNodeWithContentDescription("Collapse tool: Restore anchor tool").assertExists()
            compose.onNodeWithContentDescription("Hide agent thoughts").assertExists()
            compose.onNode(hasSetTextAction()).assertTextEquals("Owned conversation draft")
            // Leaving and returning from a real conversation detail page retains activity state.
            compose.onNodeWithContentDescription("Session menu").performClick()
            compose.onNodeWithText("Connection details").performClick()
            compose.onNodeWithContentDescription("Back").performClick()
            compose.onNodeWithText("Restore expanded thought").assertIsDisplayed()
            compose.onNodeWithContentDescription("Collapse tool: Restore anchor tool").assertExists()
            scenario.onActivity {it.finishAndRemoveTask()}
        }
    }
}
