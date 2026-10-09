package dev.acportal.presentation

import androidx.compose.material3.Surface
import androidx.compose.foundation.layout.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Production page states and explicit callbacks, without repositories or preferences. */
class FailedHostPagesUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun darkFailedReadsKeepExplicitRecovery()=check("dark")
    @Test fun lightFailedReadsKeepExplicitRecovery()=check("light")
    private fun check(mode:String) {
        var loading by mutableStateOf(true);var launchPage by mutableStateOf(false)
        var reloads=0;var backs=0
        compose.setContent {PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {
            if(launchPage)NewSessionScreen(null,null,loading,{backs++},{_,_->},{_,_->error("No launch during unavailable discovery")},onReload={reloads++})
            else HostDetailsScreen(null,{backs++},{error("No launch during unavailable discovery")},{},{reloads++},{},loading=loading)
        }}}
        compose.onNodeWithText("Connecting to host").assertIsDisplayed()
        compose.onNodeWithContentDescription("Refresh host").assertIsNotEnabled()
        compose.runOnIdle {loading=false}
        compose.onNodeWithText("Host unavailable").assertIsDisplayed()
        compose.onNodeWithText("Connecting to host").assertDoesNotExist()
        compose.onNodeWithContentDescription("Refresh host").performTouchInput {click()}
        compose.runOnIdle {assertEquals(1,reloads);launchPage=true;loading=true}
        compose.onNodeWithText("Loading connection").assertIsDisplayed()
        compose.onNodeWithText("Reload connection").assertDoesNotExist()
        compose.runOnIdle {loading=false}
        compose.onNodeWithText("Connection unavailable").assertIsDisplayed()
        compose.onNodeWithText("Reload connection").assertIsDisplayed().performTouchInput {click()}
        compose.onNodeWithContentDescription("Back").performTouchInput {click()}
        compose.runOnIdle {assertEquals(2,reloads);assertEquals(1,backs)}
    }
}
