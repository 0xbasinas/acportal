package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SavedSessionUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun cachedMessagesAndDraftRemainVisibleWithoutReplayingPendingActions() {
        val info=SessionInfo("s","goose","acp","/workspace")
        val cached=SessionState(sequence=77,processing=true,replaying=true,items=listOf(TimelineItem.Text("answer","agent","Saved answer")),permissions=mapOf("p" to Permission(JsonPrimitive("p"),buildJsonObject {putJsonObject("toolCall") {put("title","Obsolete approval")};putJsonArray("options") {}})))
        val stored=StoredSession("h","s",WireJson.encodeToString(SessionInfo.serializer(),info),WireJson.encodeToString(SessionState.serializer(),cached),draft="Unsent draft")
        var draft=stored.draft;var retries=0
        compose.setContent {PortalTheme("Dark") {androidx.compose.material3.Surface {SavedSessionScreen(info,stored,null,false,{}, {draft=it},{retries++})}}}
        compose.onNodeWithText("Saved answer").assertIsDisplayed()
        compose.onNodeWithText("Saved conversation").assertIsDisplayed()
        compose.onNodeWithText("Obsolete approval").assertDoesNotExist()
        compose.onNodeWithContentDescription("Stop turn").assertDoesNotExist()
        compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
        compose.onNode(hasSetTextAction()).assertTextEquals("Unsent draft").performTextReplacement("Updated offline draft")
        compose.runOnIdle {assertEquals("Updated offline draft",draft);assertEquals(77L,cached.sequence);assertEquals(1,cached.permissions.size);assertTrue(cached.processing)}
        compose.onNodeWithText("Connect to host").performClick()
        compose.runOnIdle {assertEquals(1,retries)}
    }
    @Test fun missingCachedHistoryDoesNotClaimItIsLoadingOrAllowSend() {
        val info=SessionInfo("s","goose","acp","/workspace")
        compose.setContent {PortalTheme {SavedSessionScreen(info,StoredSession("h","s","{}"),null,true,{}, {},{})}}
        compose.onNodeWithText("Checking connection…").assertIsNotEnabled()
        compose.onNodeWithText("No messages saved").assertIsDisplayed()
        compose.onNodeWithText("Ready to work").assertDoesNotExist()
        compose.onNodeWithContentDescription("Send message").assertIsNotEnabled()
    }
}
