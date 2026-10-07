package dev.acportal.presentation

import androidx.compose.material3.Surface
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AttachmentControlsUiTest {
    @get:Rule val compose=createComposeRule()
    private fun info(vararg caps:String)=SessionInfo("id","agent","session","workspace",initialization=buildJsonObject {putJsonObject("agentCapabilities") {putJsonObject("promptCapabilities") {caps.forEach {put(it,true)}}}})
    @Test fun unsupportedMediaActionsAreHiddenAndHostReferenceCanBeAdded() {
        var selected:PromptAttachment?=null;var version=-1L
        compose.setContent {PortalTheme {Surface {AttachmentControls(info(),true,7,{}, {item,generation->selected=item;version=generation})}}}
        compose.onNodeWithContentDescription("Add context").performClick()
        compose.onNodeWithText("Attach file").assertDoesNotExist()
        compose.onNodeWithText("Attach image").assertDoesNotExist()
        compose.onNodeWithText("Attach audio").assertDoesNotExist()
        compose.onNodeWithText("Context URI").performClick()
        compose.onNodeWithText("Context URI").performTextInput("file:///workspace/main.kt")
        compose.onNodeWithText("Add reference").performClick()
        compose.runOnIdle {assertEquals("resource_link",selected!!.content["type"].text());assertEquals(7,version)}
    }
    @Test fun advertisedAttachmentsAppearWithoutFakeAgentNames() {
        compose.setContent {PortalTheme {Surface {AttachmentControls(info("image","audio","embeddedContext"),true,0,{}, {_,_->})}}}
        compose.onNodeWithContentDescription("Add context").performClick()
        listOf("Attach file","Attach image","Attach audio","Context URI").forEach {compose.onNodeWithText(it).assertIsDisplayed()}
    }
}
