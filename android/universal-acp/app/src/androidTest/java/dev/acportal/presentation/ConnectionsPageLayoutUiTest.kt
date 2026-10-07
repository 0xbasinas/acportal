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
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConnectionsPageLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun darkCompactEmptyAndOfflineActionsRemainReachable()=check("dark")
    @Test fun lightCompactEmptyAndOfflineActionsRemainReachable()=check("light")
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
        compose.runOnIdle {assertEquals(1,adds);hosts=listOf(HostProfile("h","Offline host","https://host.example","alias","d",0,online=false,agentCount=5))}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Offline"))
        compose.onNodeWithText("Offline").assertIsDisplayed()
        compose.onNodeWithText("5 agents available",substring=true).assertDoesNotExist()
        compose.onNodeWithText("Offline host").performClick()
        compose.onNodeWithContentDescription("Refresh connections").performClick()
        compose.runOnIdle {assertEquals("h",selected);assertEquals(1,refreshes)}
    }
}
