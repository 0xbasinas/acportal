package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Rule
import org.junit.Test

class ConversationKeyboardHistoryTest {
    @get:Rule val compose=createEmptyComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(90)
    @Test fun darkNativeHistorySelectionPreservesDraftUntilExplicitReplacement()=check("dark")
    @Test fun lightNativeHistorySelectionPreservesDraftUntilExplicitReplacement()=check("light")
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val scenario=ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java)
            .putExtra("page","conversation").putExtra("mode",mode))
        fun keyboardVisible():Boolean {
            var visible=false
            scenario.onActivity {visible=it.fixtureKeyboardVisible}
            return visible
        }
        fun selectHistory() {
            compose.onNodeWithContentDescription("Add context").assertIsDisplayed().performTouchInput {click()}
            compose.onNodeWithText("Prompt history").assertIsDisplayed().performTouchInput {click()}
            compose.onNodeWithTag("history:history-fixture").assertIsDisplayed().performTouchInput {click()}
            compose.onNodeWithText("Replace draft?").assertIsDisplayed()
        }
        try {
            val composer=compose.onNode(hasSetTextAction())
            composer.performTouchInput {click()}
            compose.waitUntil(10_000) {keyboardVisible()}
            composer.assertIsFocused().performTextClearance()
            composer.performTextInput("Unsaved fixture draft")
            selectHistory()
            compose.onNodeWithText("Cancel").assertIsDisplayed().performTouchInput {click()}
            composer.assertTextEquals("Unsaved fixture draft")
            selectHistory()
            compose.onNodeWithText("Replace").assertIsDisplayed().performTouchInput {click()}
            composer.assertTextEquals("Previously submitted fixture prompt")
            composer.performTouchInput {click()}
            compose.waitUntil(10_000) {keyboardVisible()}
            androidx.test.espresso.Espresso.pressBack()
            compose.waitUntil(10_000) {!keyboardVisible()}
            composer.assertIsDisplayed().assertTextEquals("Previously submitted fixture prompt")
            compose.onNodeWithContentDescription("Send message").assertIsDisplayed().assertIsEnabled()
            // Any automatic send would fail through the fixture's production-screen callback.
        } finally {
            try {scenario.onActivity {it.finishAndRemoveTask()}}
            finally {scenario.close()}
        }
    }
}
