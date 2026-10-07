package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.storage.HostProfile
import dev.acportal.transport.ConnectionState
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.text.TextLayoutResult
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConnectionsPageLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun darkCompactEmptyAndOfflineActionsRemainReachable()=check("dark")
    @Test fun lightCompactEmptyAndOfflineActionsRemainReachable()=check("light")
    @Test fun darkCompactDetailsKeepLongValuesErrorsAndActionsReachable()=checkDetails("dark")
    @Test fun lightCompactDetailsKeepLongValuesErrorsAndActionsReachable()=checkDetails("light")
    private fun checkDetails(mode:String) {
        val address="https://"+"development-".repeat(12)+"host.example.invalid:8765"
        val error="Connection unavailable. Check the host and retry. ".repeat(5)
        var state:ConnectionState by mutableStateOf(ConnectionState.Failed(error))
        var logs=0;var reconnects=0;var disconnects=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp)) {ConnectionDetailsScreen("Development workstation ".repeat(5),address,state,{}, {logs++},{reconnects++;state=ConnectionState.Connecting},{disconnects++;state=ConnectionState.Disconnected},"Configured coding agent")}}
            }
        }
        fun reveal(text:String) {compose.onNode(hasScrollAction()).performScrollToNode(hasText(text));compose.onNodeWithText(text).assertIsDisplayed()}
        reveal(address)
        reveal("Configured coding agent")
        reveal(error)
        reveal("View connection logs");compose.onNodeWithText("View connection logs").performClick()
        reveal("Reconnect");compose.onNodeWithText("Reconnect").performClick()
        compose.onNodeWithText("Reconnect").assertIsNotEnabled()
        reveal("Disconnect");compose.onNodeWithText("Disconnect").performClick()
        compose.onNodeWithText("Disconnect").assertIsNotEnabled()
        reveal("Reconnect");compose.onNodeWithText("Reconnect").performClick()
        compose.runOnIdle {assertEquals(1,logs);assertEquals(2,reconnects);assertEquals(1,disconnects)}
    }
    private fun check(mode:String) {
        var hosts by mutableStateOf(emptyList<HostProfile>())
        var adds=0;var selected="";var refreshes=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp)) {HostsScreen(hosts,{adds++},{selected=it.id},{refreshes++})}}
            }
        }
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Add connection"))
        compose.onNodeWithText("Add connection").assertIsDisplayed().performClick()
        val longName="Offline development workstation ".repeat(4).take(128)
        compose.runOnIdle {assertEquals(1,adds);hosts=listOf(HostProfile("h",longName,"https://host.example","alias","d",0,online=false,agentCount=5))}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Offline"))
        compose.onNodeWithText("Offline").assertIsDisplayed()
        compose.onNodeWithText("5 agents available",substring=true).assertDoesNotExist()
        val layouts=mutableListOf<TextLayoutResult>()
        compose.onNodeWithText(longName).performSemanticsAction(SemanticsActions.GetTextLayoutResult) {it(layouts)}
        assertTrue("Long host name exceeds two summary lines",layouts.single().lineCount<=2)
        compose.onNodeWithText(longName).performClick()
        compose.onNodeWithContentDescription("Refresh connections").performClick()
        compose.runOnIdle {assertEquals("h",selected);assertEquals(1,refreshes)}
    }
}
