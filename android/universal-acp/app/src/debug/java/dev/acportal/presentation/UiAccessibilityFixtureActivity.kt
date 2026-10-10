package dev.acportal.presentation

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import dev.acportal.data.LiveSession
import dev.acportal.storage.HostProfile
import dev.acportal.storage.StoredSession
import dev.acportal.transport.AgentTransport
import dev.acportal.transport.ConnectionState
import dev.acportal.transport.ConnectionDiagnostic
import dev.acportal.data.HostDetails
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.serialization.json.*

/** Debug-only, non-exported native accessibility surface. All decisions are memory-only. */
@OptIn(ExperimentalLayoutApi::class)
class UiAccessibilityFixtureActivity:ComponentActivity() {
    var fixtureKeyboardVisible=false
        private set
    val fixtureActions=mutableListOf<String>()
    var fixtureDecision:String?=null
        private set
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        val mode=intent.getStringExtra("mode") ?: "dark"
        val page=intent.getStringExtra("page") ?: "shell"
        setContent {
            val keyboard=WindowInsets.isImeVisible
            SideEffect {fixtureKeyboardVisible=keyboard}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {
                if(page=="models")ModelCatalogFixture()
                else if(page=="creation")SessionConnectionScreen(listOf(HostProfile("fixture-host","Fixture computer","https://example.invalid","unused","fixture-device",0,online=true)),{fixtureActions.add("back")},{fixtureActions.add("pair")},{fixtureActions.add("choose:$it")})
                else if(page=="pairing")PairScreen(false,{fixtureActions.add("back")},{_,_,_->fixtureActions.add("pair")},scanner={_,failed,_->failed()})
                else if(page=="markdown")Column(Modifier.verticalScroll(rememberScrollState()).padding(24.dp)) {
                    ScreenHeader("Markdown reply",onBack={fixtureActions.add("back")})
                    MarkdownMessage("# Fixture answer\n\nA **clear** reply.\n\n- First fixture item\n- Second fixture item\n\n```text\nprintf fixture\n```\n\n[Read docs](https://example.invalid/docs)")
                }
                else if(page=="conversation" || page=="elicitation")ConversationFixture(page=="elicitation")
                else if(page=="logs")ConnectionLogsScreen(listOf(
                    ConnectionDiagnostic(1000,"Info","Fixture connection opened"),
                    ConnectionDiagnostic(2000,"Warning","Fixture reconnect waiting"),
                    ConnectionDiagnostic(3000,"Error","Fixture connection unavailable")
                ),{fixtureActions.add("back")},"Fixture computer",ConnectionState.Disconnected)
                else if(page.startsWith("agents"))AgentsFixture(page)
                else if(page=="recovery") {
                    var state by remember {mutableStateOf(SessionState(error="Fixture connection failed"))}
                    Column {
                        ScreenHeader("Recovery fixture")
                        SessionRecovery(state,ConnectionState.Disconnected,
                            {state=state.copy(error=null);fixtureActions.add("dismiss")},
                            {fixtureActions.add("reconnect")},{fixtureActions.add("sessions")})
                    }
                }
                else Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                    if(page=="shell") {
                        var decision by rememberSaveable {mutableStateOf("No decision")}
                        Text("Fixture decision: $decision")
                        PermissionRequest(shellPermission(),decision=="No decision") {decision=it;fixtureDecision=it}
                    } else if(page=="notices") {
                        ScreenHeader("Agent safety notices")
                        OwnToolsNotice(compact=true)
                        ToolCard(TimelineItem.Tool("write-changed","Fixture changed write","edit","completed",hostWrite=HOST_WRITE_CHANGED))
                        ToolCard(TimelineItem.Tool("write-unchanged","Fixture unchanged write","edit","completed",hostWrite=HOST_WRITE_UNCHANGED))
                    } else {
                        ScreenHeader("Activity review")
                        for((decision,layer) in listOf("allow" to "rules","deny" to "model","ask" to "rules","reviewing" to "model")) {
                            ToolCard(TimelineItem.Tool("fixture-$decision","Review $decision command","execute",if(decision=="reviewing")"pending" else if(decision=="deny")"failed" else "completed",autoReview=buildJsonObject {
                                put("decision",decision);put("layer",layer);put("reason","Fixture $decision explanation");put("shellLine","printf 'fixture-$decision'")
                            }))
                        }
                        AgentThoughtCard("fixture-thought") {Text("Fixture expanded thought")}
                    }
                }
            }}
        }
    }
    @Composable private fun AgentsFixture(page:String) {
        val saved=if(page=="agents-saved")1000L else null
        val agent=remember {AgentInfo("fixture-agent","Fixture agent",installed=true,status=if(page=="agents-error")"misconfigured" else "available",reason=if(page=="agents-error")"Fixture executable unavailable" else null)}
        var details by rememberSaveable {mutableStateOf(false)}
        if(details)RegistryAgentDetailsScreen(agent,"Fixture computer",false,
            {details=false;fixtureActions.add("agent-back")},
            {fixtureActions.add("agent-refresh")},{fixtureActions.add("new-session")},savedAt=saved)
        else RegistryAgentsScreen(HostDetails(
            HostProfile("fixture-host","Fixture computer","https://example.invalid","unused","fixture-device",0),
            listOf(agent),emptyList(),emptyList(),emptyList()),
            {fixtureActions.add("back")},{fixtureActions.add("agents-refresh")},
            {details=true;fixtureActions.add("inspect:$it")},savedAt=saved)
    }
    @Composable private fun ConversationFixture(elicitation:Boolean=false) {
        val info=remember {SessionInfo("native-conversation","fixture-agent","fixture-acp","/workspace/fixture")}
        val live=remember {
            val transport=object:AgentTransport {
                override suspend fun connect() {error("No fixture connection")}
                override suspend fun disconnect() {}
                override suspend fun send(message:ByteArray) {error("No fixture submissions")}
                override fun messages()=emptyFlow<ByteArray>()
                override fun connectionState()=flowOf(ConnectionState.Connected)
            }
            val items=(0..60).map {index->when(index) {
                25->TimelineItem.Tool("restore-tool","Restore anchor tool","execute","completed",autoReview=buildJsonObject {
                    put("decision","allow");put("layer","rules");put("reason","Restore tool explanation")
                })
                26->TimelineItem.Text("restore-thought","thought","Restore expanded thought")
                28->TimelineItem.Text("history-fixture","user","Previously submitted fixture prompt",promptText="Previously submitted fixture prompt")
                else->TimelineItem.Text("restore-text-$index","agent","Conversation anchor $index\n"+"Fixture retained history. ".repeat(5))
            }}
            val question=Permission(JsonPrimitive("fixture-question"),buildJsonObject {
                put("mode","form");put("message","Fixture lifecycle question")
                putJsonObject("requestedSchema") {put("type","object");putJsonObject("properties") {putJsonObject("name") {put("type","string");put("title","Fixture name")}};putJsonArray("required") {add("name")}}
            },ELICITATION_METHOD)
            LiveSession(HostProfile("fixture-host","Fixture","https://example.invalid","fixture-alias","fixture-device",0),info,transport,SessionState(items=items,permissions=if(elicitation)mapOf(question.id.toString() to question) else emptyMap())).also {it.connection.value=ConnectionState.Connected}
        }
        SessionScreen(live,StoredSession("fixture-host",info.id,WireJson.encodeToString(SessionInfo.serializer(),info),draft="Owned conversation draft"),
            onBack={},onPrompt={_,_->error("No fixture prompt")},onCancel={},onPermission={_,_->error("No fixture permission")},onConfigure={_,_->error("No fixture configuration")},onDraft={},
            onAddAttachment={_,_->error("No fixture attachments")},onRemoveAttachment={},onElicitation={_,action,_->fixtureActions.add("elicitation:$action")})
    }
    @Composable private fun ModelCatalogFixture() {
        val models=remember {buildJsonObject {
            put("currentModelId"," model-0 ")
            putJsonArray("availableModels") {repeat(700) {index->add(buildJsonObject {put("modelId"," model-$index ");put("name","Model $index")})}}
        }}
        SessionOptionsScreen(SessionInfo("fixture","fixture-agent","fixture-acp","/fixture"),SessionState(models=models),true,{fixtureActions.add("back")},{_,params->fixtureActions.add("model:${params["modelId"].text()}")})
    }
    private fun shellPermission()=Permission(JsonPrimitive("native-fixture"),buildJsonObject {
        putJsonObject("_meta") {put("acpdSource","host-shell-command")}
        putJsonObject("toolCall") {
            put("title","Review fixture command")
            putJsonObject("rawInput") {put("shellLine","printf '%s' 'review the complete fixture command'");put("shell","/bin/sh -c");put("cwd","/workspace/fixture")}
        }
        putJsonArray("options") {
            add(buildJsonObject {put("optionId","fixture-allow");put("name","Run this shell line once");put("kind","allow_once")})
            add(buildJsonObject {put("optionId","fixture-deny");put("name","Deny");put("kind","reject_once")})
        }
    })
}
