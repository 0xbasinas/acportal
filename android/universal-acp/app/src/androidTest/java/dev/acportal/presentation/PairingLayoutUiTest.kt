package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean

class PairingLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun runningRequestDisablesEveryFieldAndDuplicateSubmission() {
        compose.setContent {PortalTheme {PairScreen(true,{}, {_,_,_->fail("Submitted while busy")},initialAddress="https://host.example",initialLabel="Workstation")}}
        listOf("Connection name","Address","Pairing code").forEach {label->scroll(label);compose.onNodeWithText(label).assertIsNotEnabled()}
        scroll("Connecting…")
        compose.onNodeWithText("Connecting…").assertIsNotEnabled()
    }
    @Test fun keyboardSubmissionAndFailedRequestPreserveFieldsUntilExplicitRetry()=checkKeyboard("dark")
    @Test fun lightKeyboardSubmissionAndFailedRequestPreserveFieldsUntilExplicitRetry()=checkKeyboard("light")
    @OptIn(ExperimentalLayoutApi::class)
    private fun checkKeyboard(mode:String) {
        if(androidx.test.platform.app.InstrumentationRegistry.getArguments().getString("requireLargeFont")=="true")assertTrue(androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext.resources.configuration.fontScale>=1.9f)
        val keyboard=AtomicBoolean(false)
        var loading by mutableStateOf(false)
        var failure by mutableStateOf<String?>(null)
        val requests=mutableListOf<List<String>>()
        compose.setContent {
            val visible=WindowInsets.isImeVisible
            SideEffect {keyboard.set(visible)}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {Column {
                failure?.let {ConnectionError(it,{failure=null})}
                PairScreen(loading,{}, {address,code,name->requests+=listOf(address,code,name);loading=true},initialAddress="https://host.example",initialLabel="Workstation")
            }}}
        }
        scroll("Pairing code")
        compose.onNodeWithText("Pairing code").performClick().performTextInput("fixture-code")
        compose.waitUntil(10_000) {keyboard.get()}
        scroll("Pair connection")
        compose.onNodeWithText("Pair connection").assertIsDisplayed()
        assertTrue(keyboard.get())
        compose.onNodeWithText("Pair connection").performClick()
        compose.onNodeWithText("Connecting…").assertIsNotEnabled()
        compose.runOnIdle {assertEquals(listOf(listOf("https://host.example","FIXTURE-CODE","Workstation")),requests);loading=false;failure="Pairing failed. Check the code and try again."}
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        scroll("Connection name");compose.onNodeWithText("Connection name").assertTextContains("Workstation")
        scroll("Address");compose.onNodeWithText("Address").assertTextContains("https://host.example")
        scroll("Pairing code");compose.onNodeWithText("Pairing code").assertTextContains("FIXTURE-CODE")
        compose.runOnIdle {assertEquals(1,requests.size)}
        compose.onNodeWithContentDescription("Dismiss error").performClick()
        scroll("Pair connection")
        compose.onNodeWithText("Pair connection").performClick()
        compose.runOnIdle {assertEquals(2,requests.size)}
    }
    private fun scroll(label:String) {
        compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(label))
    }
}
