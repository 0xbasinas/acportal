package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.WorkspaceAccess
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class WorkspaceAccessLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkAccessKeepsPolicyAndSaveReachable()=checkEditor("dark")
    @Test fun compactLightAccessKeepsPolicyAndSaveReachable()=checkEditor("light")

    private fun checkEditor(mode:String) {
        var saved:WorkspaceAccess?=null
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("access-fixture")) {
                    WorkspaceAccessScreen("Workstation","/projects/app",WorkspaceAccess(),WorkspaceAccess(),false,{}, {saved=it})
                }}
            }
        }
        listOf("Write files","Terminal").forEach {label->
            compose.onNode(hasScrollAction()).performScrollToNode(hasContentDescription(label))
            compose.onNodeWithContentDescription(label).assertIsDisplayed().performClick()
        }
        val current="Current session: Read on · Write on · Terminal on"
        compose.onNode(hasScrollAction()).performScrollToNode(hasText(current))
        compose.onNodeWithText(current).assertIsDisplayed()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Save for next session"))
        val save=compose.onNodeWithText("Save for next session")
        val action=save.getUnclippedBoundsInRoot()
        val window=compose.onNodeWithTag("access-fixture").getUnclippedBoundsInRoot()
        assertTrue("Save extends outside the compact window",action.top>=window.top && action.bottom<=window.bottom)
        save.assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals(WorkspaceAccess(true,false,false),saved)}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText(current))
        compose.onNodeWithText(current).assertIsDisplayed()
    }

    @Test fun failedWorkspaceLoadOffersExplicitReloadAndBack() {
        var reloads=0;var backs=0
        compose.setContent {PortalTheme {WorkspaceAccessListScreen(null,null,false,{backs++},{fail("Browse without folders")},{fail("Open without folders")},onReload={reloads++})}}
        compose.onNodeWithText("Workspaces unavailable").assertIsDisplayed()
        compose.onNodeWithText("Loading workspaces").assertDoesNotExist()
        compose.onNodeWithText("Browse folders").assertDoesNotExist()
        compose.runOnIdle {assertEquals(0,reloads)}
        compose.onNodeWithText("Reload workspaces").assertIsDisplayed().performClick()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle {assertEquals(1,reloads);assertEquals(1,backs)}
    }

    @Test fun workspaceLoadingDoesNotOfferFailureActions() {
        compose.setContent {PortalTheme {WorkspaceAccessListScreen(null,null,true,{},{},{},onReload={fail("Reload during initial read")})}}
        compose.onNodeWithText("Loading workspaces").assertIsDisplayed()
        compose.onNodeWithText("Workspaces unavailable").assertDoesNotExist()
        compose.onNodeWithText("Reload workspaces").assertDoesNotExist()
    }
}
