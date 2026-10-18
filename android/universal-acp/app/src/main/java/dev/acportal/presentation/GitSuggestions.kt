package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.AgentInfo

/**
 * Text an agent on the host writes for a commit or a pull request: the agents the host can start for
 * it, and the two requests. The agent works in a throwaway session on the host, so nothing lands in
 * a conversation, and its text only fills the box: nothing is committed or opened until the person
 * presses the button.
 */
class GitSuggestions(
    val agents:List<AgentInfo>,
    val commit:suspend (agentId:String)->Result<String>,
    val pullRequest:suspend (agentId:String,base:String)->Result<Pair<String,String>>,
)

/** One button when a single agent is available, a short list to choose from when there are several. */
@Composable internal fun SuggestButton(agents:List<AgentInfo>,busy:Boolean,tag:String,modifier:Modifier=Modifier,onPick:(AgentInfo)->Unit) {
    var choosing by remember {mutableStateOf(false)}
    if(agents.isEmpty())return
    OutlinedButton({if(agents.size==1)onPick(agents.first()) else choosing=true},modifier.heightIn(min=48.dp).testTag(tag),enabled=!busy) {
        Text(if(busy)"Writing…" else if(agents.size==1)"Write with ${agents.first().name}" else "Write with an agent…",maxLines=1)
    }
    if(choosing)AlertDialog(onDismissRequest={choosing=false},title={Text("Which agent writes it?")},text={
        Column {agents.forEach {agent->TextButton({choosing=false;onPick(agent)},Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("suggest-${agent.id}")) {Text(agent.name,Modifier.fillMaxWidth())}}}
    },confirmButton={},dismissButton={TextButton({choosing=false}) {Text("Cancel")}})
}
