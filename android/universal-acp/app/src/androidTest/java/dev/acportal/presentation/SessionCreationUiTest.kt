package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.HostDetails
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionCreationUiTest {
    @get:Rule val compose=createComposeRule()
    private val host=HostProfile("h","Workstation","https://host.example","alias","d",0,online=true)
    @Test fun connectionSelectionIncludesOfflineWithoutStarting() {
        var selected=""
        compose.setContent {PortalTheme {SessionConnectionScreen(listOf(host,host.copy(id="offline",label="Laptop",online=false)),{},{},{selected=it})}}
        compose.onNodeWithText("Offline · check connection").assertIsDisplayed()
        compose.onNodeWithText("Laptop").performClick()
        compose.runOnIdle {assertEquals("offline",selected)}
    }
    @Test fun noHostsOfferPairingAndBack() {
        var paired=false;var back=false
        compose.setContent {PortalTheme("light") {SessionConnectionScreen(emptyList(),{back=true},{paired=true},{fail("No hosts")})}}
        compose.onNodeWithText("Pair your first connection").assertIsDisplayed()
        compose.onNodeWithText("Pair a connection").performClick()
        compose.onNodeWithContentDescription("Back").performClick()
        compose.runOnIdle {assertTrue(paired);assertTrue(back)}
    }
    @Test fun approvedFormStartsOnlyExplicitlyAndChangeDoesNotCreate() {
        var calls=0;var change=false
        val details=HostDetails(host,listOf(AgentInfo("goose","Goose",installed=true)),listOf(Workspace("/work","Project")),emptyList(),emptyList())
        compose.setContent {PortalTheme {NewSessionScreen(details,null,false,{}, {_,_->},{agent,path->assertEquals("goose",agent);assertEquals("/work",path);calls++},onChangeConnection={change=true})}}
        compose.onNodeWithText("Change").performClick()
        compose.runOnIdle {assertTrue(change);assertEquals(0,calls)}
        compose.onNodeWithText("Goose").performClick()
        compose.onNodeWithText("Close").performClick()
        compose.runOnIdle {assertEquals(0,calls)}
        compose.onNodeWithText("Start session").performClick()
        compose.runOnIdle {assertEquals(1,calls)}
    }
    @Test fun failedDiscoveryKeepsExplicitRecovery() {
        var retry=0;var change=0
        compose.setContent {PortalTheme("light") {NewSessionScreen(null,null,false,{}, {_,_->},{_,_->fail("No discovery")},onReload={retry++},onChangeConnection={change++})}}
        compose.onNodeWithText("Reload connection").performClick()
        compose.onNodeWithText("Choose another connection").performClick()
        compose.onNodeWithText("Start session").assertDoesNotExist()
        compose.runOnIdle {assertEquals(1,retry);assertEquals(1,change)}
    }
}
