package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.test.espresso.Espresso
import dev.acportal.protocol.*
import dev.acportal.storage.PortalSettings
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/**
 * Back, dialog and keyboard behaviour for runbook §8. System Back (the same dispatcher
 * that receives predictive Back) must close the top-most sheet or dialog only, and the
 * first Back with the keyboard open must hide the keyboard without leaving the page.
 * The predictive-Back animation itself is checked by hand with the gesture.
 */
class MotionInputFixtureTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)

    @Test fun backClosesThePromptHistorySheetWithoutSelecting() {
        var open by mutableStateOf(true)
        var dismissed=0
        val selected=mutableListOf<String>()
        compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
            if(open)PromptHistorySheet(listOf(PromptHistoryEntry("1","Run the tests")),{dismissed++;open=false}) {selected+=it}
        }}}
        compose.onNodeWithText("Prompt history").assertIsDisplayed()
        Espresso.pressBack()
        compose.waitUntil(5_000) {!open}
        compose.runOnIdle {assertEquals(1,dismissed);assertTrue(selected.isEmpty())}
    }

    @Test fun backClosesASettingsDialogAndKeepsThePage() {
        compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {SettingsScreen(PortalSettings(),{},{})}}}
        compose.onNodeWithText("Appearance").performClick()
        compose.onNodeWithText("Done").assertIsDisplayed()
        Espresso.pressBack()
        compose.onNodeWithText("Done").assertDoesNotExist()
        compose.onNodeWithText("Settings").assertIsDisplayed()
    }

    @Test fun firstBackHidesTheKeyboardAndKeepsTheTypedText() {
        var imeVisible={false}
        compose.setContent {
            val view=LocalView.current
            imeVisible={ViewCompat.getRootWindowInsets(view)?.isVisible(WindowInsetsCompat.Type.ime())==true}
            PortalTheme {Surface(Modifier.fillMaxSize().imePadding()) {PairScreen(false,{error("Back must not leave while the keyboard is open")},{_,_,_->})}}
        }
        val field=compose.onAllNodes(hasSetTextAction()).onFirst()
        field.performClick()
        field.performTextInput("192.168.1.20:8765")
        compose.waitUntil(5_000) {imeVisible()}
        Espresso.pressBack()
        compose.waitUntil(5_000) {!imeVisible()}
        compose.onNodeWithText("192.168.1.20:8765",substring=true).assertExists()
    }
}
