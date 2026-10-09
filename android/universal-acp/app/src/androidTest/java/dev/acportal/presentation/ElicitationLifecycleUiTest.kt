package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Actual modal and keyboard; all answer callbacks stay in the owned Activity. */
class ElicitationLifecycleUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(90)
    @Test fun darkDismissalAndRecreationNeverAnswerAutomatically()=check("dark")
    @Test fun lightDismissalAndRecreationNeverAnswerAutomatically()=check("light")
    private fun check(mode:String) {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        if(InstrumentationRegistry.getArguments().getString("requireLargeFont")=="true")assertTrue(context.resources.configuration.fontScale>=1.9f)
        ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java)
            .putExtra("page","elicitation").putExtra("mode",mode)).use {scenario->
            compose.onNodeWithText("Fixture lifecycle question").assertIsDisplayed()
            val field=compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("elicitation-field:name")))
            field.performScrollTo().performTouchInput {click()}.performTextInput("Transient fixture answer")
            compose.waitUntil(10_000) {var visible=false;scenario.onActivity {visible=it.fixtureKeyboardVisible};visible}
            androidx.test.espresso.Espresso.pressBack()
            compose.onNodeWithText("Fixture lifecycle question").assertExists()
            scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            androidx.test.espresso.Espresso.pressBack()
            compose.onNodeWithTag("elicitation-form").assertDoesNotExist()
            scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            compose.onNodeWithText("Answer the agent's question").performClick()
            compose.onNodeWithTag("elicitation-form").assertExists()
            scenario.recreate()
            compose.waitForIdle()
            compose.onNodeWithText("Fixture lifecycle question").assertExists()
            compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("elicitation-field:name"))).assert(SemanticsMatcher.expectValue(
                androidx.compose.ui.semantics.SemanticsProperties.EditableText,androidx.compose.ui.text.AnnotatedString("")))
            scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            compose.onNodeWithTag("elicitation-decline").performScrollTo().performTouchInput {click()}
            scenario.onActivity {assertEquals(listOf("elicitation:decline"),it.fixtureActions);it.finishAndRemoveTask()}
        }
    }
}
