package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.HostDetails
import dev.acportal.data.LiveSessionActivity
import androidx.compose.runtime.mutableStateOf
import dev.acportal.protocol.*
import dev.acportal.storage.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SessionSummaryUiTest {
    @get:Rule val compose=createComposeRule()
    private val host=HostProfile("h","Fixture host","https://host.example","alias","d",0,online=true,agentCount=2)
    @Test fun onlineConnectionsShowActualAgentAvailability() {
        compose.setContent {PortalTheme {HostsScreen(listOf(host,host.copy(id="offline",label="Offline host",online=false,agentCount=7)),{},{},{})}}
        compose.onNodeWithText("Online · 2 agents available").assertIsDisplayed()
        compose.onNodeWithText("Offline").assertIsDisplayed()
        compose.onNodeWithText("7 agents available",substring=true).assertDoesNotExist()
    }
    @Test fun selectingARecentWorkspaceKeepsItsAgentCombination() {
        var selection:Pair<String,String>?=null
        val details=HostDetails(host,listOf(AgentInfo("one","Agent one",installed=true),AgentInfo("two","Agent two",installed=true)),listOf(Workspace("/projects","Projects")),emptyList(),listOf(RecentWorkspace("h","/projects/recent","two")))
        compose.setContent {PortalTheme {NewSessionScreen(details,null,false,{},{_,_->},{agent,path->selection=agent to path})}}
        compose.onNodeWithTag("recent:two:/projects/recent").performClick()
        compose.onNodeWithText("Start session").performClick()
        compose.runOnIdle {assertEquals("two" to "/projects/recent",selection)}
    }
    @Test fun sessionRowsShowReportedTitlesAndActualTimeSource() {
        val info=SessionInfo("s","mock","acp","/projects/app")
        val now=System.currentTimeMillis()
        val state=SessionState(sessionInfo=buildJsonObject {put("title","Reported title");put("updatedAt",java.time.Instant.ofEpochMilli(now-10_000).toString())})
        val reported=StoredSession("h","s",WireJson.encodeToString(SessionInfo.serializer(),info),WireJson.encodeToString(SessionState.serializer(),state))
        val local=StoredSession("h","local",WireJson.encodeToString(SessionInfo.serializer(),info.copy(id="local")),updatedAt=now)
        compose.setContent {PortalTheme {SessionsScreen(listOf(reported,local),listOf(host),{},{},{},{},{})}}
        compose.onNodeWithText("Reported title").assertIsDisplayed()
        compose.onNodeWithText("Agent updated Just now").assertIsDisplayed()
        compose.onNodeWithText("Saved Just now").assertIsDisplayed()
    }
    @Test fun rowsFollowIndependentLiveActivitiesInsteadOfCachedProcessing() {
        val info=SessionInfo("one","mock","acp","/projects/app")
        val cached=WireJson.encodeToString(SessionState.serializer(),SessionState(processing=true))
        val sessions=listOf("one","two","old").map {id->StoredSession("h",id,WireJson.encodeToString(SessionInfo.serializer(),info.copy(id=id,status=if(id=="old")"running" else "ready")),cached)}
        val activities=mutableStateOf(mapOf("h/one" to LiveSessionActivity.WORKING,"h/two" to LiveSessionActivity.WAITING_APPROVAL))
        compose.setContent {PortalTheme {SessionsScreen(sessions,listOf(host),{},{},{},{},{},liveActivities=activities.value)}}
        compose.onNodeWithText("mock · Working").assertIsDisplayed()
        compose.onNodeWithText("mock · Waiting for approval").assertIsDisplayed()
        compose.onNodeWithText("mock · Last reported: Working").assertIsDisplayed()
        compose.runOnIdle {activities.value=activities.value+("h/one" to LiveSessionActivity.DISCONNECTED)}
        compose.onNodeWithText("mock · Disconnected").assertIsDisplayed()
        compose.onNodeWithText("mock · Waiting for approval").assertIsDisplayed()
        compose.onNodeWithText("mock · Working").assertDoesNotExist()
    }
}
