package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.WorkspaceAccess
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class WorkspaceAccessUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun savingFutureAccessDoesNotChangeCurrentSessionDisplay() {
        var saved:WorkspaceAccess?=null
        compose.setContent {PortalTheme {WorkspaceAccessScreen("Workstation","/projects/app",WorkspaceAccess(),WorkspaceAccess(),false,{}, {saved=it})}}
        compose.onNodeWithContentDescription("Write files").performClick()
        compose.onNodeWithContentDescription("Terminal").performClick()
        compose.onNodeWithText("Save for next session").performClick()
        compose.runOnIdle {assertEquals(WorkspaceAccess(true,false,false),saved)}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Current session: Read on · Write on · Terminal on"))
        compose.onNodeWithText("Current session: Read on · Write on · Terminal on").assertIsDisplayed()
    }
    @Test fun loadingPreventsPolicyChangesAndSave() {
        compose.setContent {PortalTheme {WorkspaceAccessScreen("Workstation","/projects/app",WorkspaceAccess(),null,true,{}, {fail("Saved while loading")})}}
        listOf("Read files","Write files","Terminal").forEach {compose.onNodeWithContentDescription(it).assertIsNotEnabled()}
        compose.onNodeWithText("Save access").assertIsNotEnabled()
    }
}
