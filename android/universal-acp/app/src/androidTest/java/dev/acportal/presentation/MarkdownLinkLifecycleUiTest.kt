package dev.acportal.presentation

import android.content.Intent
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Link confirmation is transient: recreation closes it and never opens a browser by itself. */
class MarkdownLinkLifecycleUiTest {
    @get:Rule val compose=createEmptyComposeRule()
    @Test fun darkConfirmationClosesOnRecreationWithoutNavigating()=check("dark")
    @Test fun lightConfirmationClosesOnRecreationWithoutNavigating()=check("light")
    private fun openLink() {
        compose.waitUntil(10_000) {compose.onAllNodes(hasText("Read docs") and hasClickAction()).fetchSemanticsNodes().isNotEmpty()}
        compose.onNode(hasText("Read docs") and hasClickAction()).performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnClick)
    }
    private fun check(mode:String) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        ActivityScenario.launch<UiAccessibilityFixtureActivity>(Intent(context,UiAccessibilityFixtureActivity::class.java).putExtra("mode",mode).putExtra("page","markdown")).use {scenario->
            openLink()
            compose.onNodeWithText("Open link?").assertIsDisplayed()
            compose.onNodeWithText("https://example.invalid/docs").assertIsDisplayed()
            scenario.recreate()
            compose.waitForIdle()
            compose.onNodeWithText("Open link?").assertDoesNotExist()
            compose.waitUntil(10_000) {compose.onAllNodes(hasText("Read docs") and hasClickAction()).fetchSemanticsNodes().isNotEmpty()}
            scenario.onActivity {assertTrue("Recreation must not act on the link",it.fixtureActions.isEmpty())}
            // The link can be reviewed again and cancelled explicitly.
            openLink()
            compose.onNodeWithText("Open link?").assertIsDisplayed()
            compose.onNodeWithText("Cancel").performSemanticsAction(androidx.compose.ui.semantics.SemanticsActions.OnClick)
            compose.onNodeWithText("Open link?").assertDoesNotExist()
        }
    }
}
