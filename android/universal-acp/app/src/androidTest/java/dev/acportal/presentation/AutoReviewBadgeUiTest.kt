package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test

class AutoReviewBadgeUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    @Test fun darkEveryOutcomeKeepsItsReasonAndIndependentExpansion()=check("dark")
    @Test fun lightEveryOutcomeKeepsItsReasonAndIndependentExpansion()=check("light")
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java)
            .putExtra("page","tools").putExtra("mode",mode)).use {scenario->
            for((decision,label) in listOf("allow" to "Auto-allowed by rules","deny" to "Auto-denied by model",
                "ask" to "Sent to you by rules review","reviewing" to "Being reviewed by model")) {
                compose.onNodeWithText(label).performScrollTo().assertIsDisplayed()
                compose.onNodeWithContentDescription("Expand tool: Review $decision command").performScrollTo().performClick()
                compose.onNodeWithText("Reason: Fixture $decision explanation").performScrollTo().assertIsDisplayed()
                compose.onNodeWithText("printf 'fixture-$decision'").performScrollTo().assertIsDisplayed()
            }
            scenario.recreate()
            compose.waitForIdle()
            for(decision in listOf("allow","deny","ask","reviewing")) {
                compose.onNodeWithContentDescription("Collapse tool: Review $decision command").assertExists()
            }
            compose.onNodeWithContentDescription("Collapse tool: Review ask command").performScrollTo().performClick()
            compose.onNodeWithText("Reason: Fixture ask explanation").assertDoesNotExist()
            compose.onNodeWithContentDescription("Collapse tool: Review allow command").assertExists()
            scenario.onActivity {it.finishAndRemoveTask()}
        }
    }
}
