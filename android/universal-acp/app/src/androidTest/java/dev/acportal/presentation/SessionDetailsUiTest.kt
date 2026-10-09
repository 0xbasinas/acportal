package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.material3.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionDetailsUiTest {
    @get:Rule val compose=createComposeRule()
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun largeLegacyModelsAreLazySearchableAndUseExactIds()=largeModels("dark",false)
    @Test fun largeGroupedConfigModelsAreLazySearchableAndUseExactIds()=largeModels("light",true)
    private fun largeModels(theme:String,grouped:Boolean) {
        val options=buildJsonArray {repeat(700) {index->add(buildJsonObject {put("value"," model-$index ");put("name","Model $index")})}}
        val state=if(grouped)SessionState(configOptions=buildJsonArray {add(buildJsonObject {
            put("id","engine");put("name","Model");put("type","select");put("currentValue"," model-0 ")
            putJsonArray("options") {add(buildJsonObject {put("name","Unified route");put("options",options)})}
        })}) else SessionState(models=buildJsonObject {
            put("currentModelId"," model-0 ")
            putJsonArray("availableModels") {options.forEach {value->val item=value.objectValue();add(JsonObject(item+("modelId" to item.getValue("value"))))}}
        })
        val enabled=androidx.compose.runtime.mutableStateOf(true)
        var calls=0;var selected="";var method=""
        compose.setContent {PortalTheme(theme) {SessionOptionsScreen(SessionInfo("s","agent","acp","/w"),state,enabled.value,{}, {m,p->calls++;method=m;selected=p[if(grouped)"value" else "modelId"].text()})}}
        compose.onNodeWithText("Model 0").performClick()
        compose.onNodeWithText("Options: 700").assertIsDisplayed()
        assertTrue("Catalog eagerly composed",compose.onAllNodes(hasText("Model ",substring=true)).fetchSemanticsNodes().size<40)
        compose.onNodeWithTag("config-choice-list").performScrollToNode(hasText("Model 699"))
        compose.onNodeWithText("Model 699").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle {assertEquals(0,calls)}
        compose.onNodeWithText("Model 0").performClick()
        compose.onNode(hasSetTextAction()).performTextInput("model-699")
        compose.onNodeWithText("Options: 1").assertIsDisplayed()
        compose.onNodeWithText("Model 699").performClick()
        compose.runOnIdle {assertEquals(1,calls);assertEquals(" model-699 ",selected);assertEquals(if(grouped)"session/set_config_option" else "session/set_model",method)}
        compose.onNodeWithText("Model 0").performClick()
        compose.runOnIdle {enabled.value=false}
        compose.onNodeWithTag("config-choice-list").assertDoesNotExist()
        compose.runOnIdle {assertEquals(1,calls)}
    }
    @Test fun headerStatusAlignsWithSubtitleAndStaysAboveBody() {
        compose.setContent {PortalTheme {androidx.compose.foundation.layout.Column {
            ScreenHeader("Long conversation title","Workspace · goose",{},action={TextButton({}) {Text("Menu")}},status="Working")
            Text("Conversation body")
        }}}
        val subtitle=compose.onNodeWithText("Workspace · goose").getUnclippedBoundsInRoot()
        val status=compose.onNodeWithText("Working").getUnclippedBoundsInRoot()
        val body=compose.onNodeWithText("Conversation body").getUnclippedBoundsInRoot()
        assertEquals(subtitle.left,status.left)
        assertTrue(status.top>=subtitle.bottom)
        assertTrue(status.bottom<body.top)
    }
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
