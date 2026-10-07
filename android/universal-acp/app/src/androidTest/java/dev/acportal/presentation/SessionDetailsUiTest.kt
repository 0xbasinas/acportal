package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionDetailsUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun groupedConfigurationUsesAdvertisedIdsAndReplacesLegacyModes() {
        val config=WireJson.parseToJsonElement("""[{"id":"custom-engine","name":"Engine","type":"select","currentValue":"quick","options":[{"group":"available","name":"Available models","options":[{"value":"quick","name":"Quick"},{"value":"thorough","name":"Thorough"}]}]}]""").arrayValue()
        val modes=WireJson.parseToJsonElement("""{"availableModes":[{"id":"legacy","name":"Legacy mode"}]}""").objectValue()
        var method="";var params=JsonObject(emptyMap())
        compose.setContent {PortalTheme {SessionOptionsScreen(SessionInfo("id","custom-agent","session","workspace"),SessionState(configOptions=config,modes=modes),true,{}, {m,p->method=m;params=p})}}
        compose.onNodeWithText("Legacy mode").assertDoesNotExist()
        compose.onNodeWithText("Quick").performClick()
        compose.onNodeWithText("Available models").assertIsDisplayed()
        compose.onNodeWithText("Thorough").performClick()
        compose.runOnIdle {assertEquals("session/set_config_option",method);assertEquals("custom-engine",params["configId"].text());assertEquals("thorough",params["value"].text())}
    }
    @Test fun unavailableCapabilitiesAreReportedWithoutInventedSupport() {
        compose.setContent {PortalTheme {AgentDetailsScreen(SessionInfo("id","custom-agent","session","workspace"),{})}}
        compose.onNodeWithText("Agent details").assertIsDisplayed()
        compose.onAllNodesWithText("Supported").assertCountEquals(0)
        compose.onAllNodesWithText("Unavailable").assertCountEquals(6)
    }
    @Test fun disconnectedOptionsCannotSendASelection() {
        val config=WireJson.parseToJsonElement("""[{"id":"engine","name":"Engine","type":"select","currentValue":"quick","options":[{"value":"quick","name":"Quick"}]}]""").arrayValue()
        compose.setContent {PortalTheme {SessionOptionsScreen(SessionInfo("id","custom","session","workspace"),SessionState(configOptions=config),false,{}, {_,_->fail("Disconnected selection")})}}
        compose.onNodeWithText("Quick").assertIsNotEnabled()
    }
}
