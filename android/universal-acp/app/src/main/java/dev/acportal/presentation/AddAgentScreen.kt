package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import dev.acportal.data.AgentClient
import dev.acportal.data.AgentRegistration
import dev.acportal.protocol.AgentCheck
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/** Agent id from a display name: lower-case letters, digits and hyphens, at most 64 characters. */
internal fun agentIdFromName(name:String):String =
    name.lowercase().map {if(it in 'a'..'z' || it in '0'..'9')it else '-'}.joinToString("").replace(Regex("-+"),"-").trim('-').take(64)

/** NAME=value lines to a map. Returns the first invalid line instead when one does not parse. */
internal fun parseEnvironment(text:String):Pair<Map<String,String>,String?> {
    val result=linkedMapOf<String,String>()
    for(line in text.lines().map {it.trim()}.filter {it.isNotEmpty()}) {
        val index=line.indexOf('=')
        if(index<=0 || line.substring(0,index).any {it.isWhitespace()})return emptyMap<String,String>() to line
        result[line.substring(0,index)]=line.substring(index+1)
    }
    return result to null
}

/**
 * Register an agent CLI on the connected computer. "Check" runs "<command> --version" there once,
 * so a typo or a missing install shows up before anything is saved. Saving needs a found program.
 */
@Composable fun AddAgentScreen(client:AgentClient,hostLabel:String,onBack:()->Unit,onSaved:()->Unit) {
    val scope=rememberCoroutineScope()
    var name by rememberSaveable {mutableStateOf("")}
    var command by rememberSaveable {mutableStateOf("")}
    var args by rememberSaveable {mutableStateOf("")}
    var environment by rememberSaveable {mutableStateOf("")}
    var ownTools by rememberSaveable {mutableStateOf(false)}
    var checking by remember {mutableStateOf(false)}
    var saving by remember {mutableStateOf(false)}
    var check by remember {mutableStateOf<AgentCheck?>(null)}
    var error by remember {mutableStateOf<String?>(null)}
    val (env,badEnvironment)=remember(environment) {parseEnvironment(environment)}
    val id=agentIdFromName(name)
    val ready=name.isNotBlank() && id.isNotEmpty() && command.isNotBlank() && badEnvironment==null
    fun changed() {check=null;error=null}
    fun run(block:suspend ()->Unit,busy:(Boolean)->Unit) {
        busy(true);error=null
        scope.launch {
            try {block()}
            catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {error=failure.message ?: "That did not work. Check the connection and try again."}
            busy(false)
        }
    }
    Column(Modifier.fillMaxSize().imePadding()) {
        ScreenHeader("Add agent",hostLabel,onBack)
        Column(Modifier.weight(1f).testTag("add-agent-body").verticalScroll(rememberScrollState()).padding(horizontal=24.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text("Register the command-line agent you want to use on $hostLabel. It runs on that computer, with that computer's files.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedTextField(name,{name=it.take(64);changed()},Modifier.fillMaxWidth().testTag("agent-name"),label={Text("Name")},placeholder={Text("Goose")},singleLine=true,keyboardOptions=KeyboardOptions(imeAction=ImeAction.Next),supportingText={if(id.isNotEmpty())Text("ID: $id")})
            OutlinedTextField(command,{command=it.take(1024);changed()},Modifier.fillMaxWidth().testTag("agent-command"),label={Text("Command")},placeholder={Text("goose")},singleLine=true,keyboardOptions=KeyboardOptions(autoCorrectEnabled=false,imeAction=ImeAction.Next),supportingText={Text("A program name on that computer's PATH, or its full path.")})
            OutlinedTextField(args,{args=it.take(4096);error=null},Modifier.fillMaxWidth().testTag("agent-args"),label={Text("Arguments")},placeholder={Text("acp")},minLines=2,maxLines=6,keyboardOptions=KeyboardOptions(autoCorrectEnabled=false),supportingText={Text("One argument per line. Most agents need one that starts ACP mode.")})
            OutlinedTextField(environment,{environment=it.take(8192);changed()},Modifier.fillMaxWidth().testTag("agent-env"),label={Text("Environment (optional)")},placeholder={Text("NAME=value")},minLines=2,maxLines=6,isError=badEnvironment!=null,keyboardOptions=KeyboardOptions(autoCorrectEnabled=false),supportingText={Text(if(badEnvironment!=null)"Use NAME=value, one per line." else "One NAME=value per line. Saved on that computer; avoid secrets you can set in the agent's own configuration.")})
            Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text("Uses its own tools",style=MaterialTheme.typography.bodyLarge)
                    Text("Turn on if it edits files or runs commands itself, outside this app's approvals. The app will warn you.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Switch(ownTools,{ownTools=it},Modifier.semantics {contentDescription="Uses its own tools"})
            }
            check?.let {result->
                Surface(shape=MaterialTheme.shapes.medium,color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.fillMaxWidth().testTag("agent-check-result")) {
                    Column(Modifier.padding(16.dp),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                        if(!result.found) {
                            Text("Not found on this computer",style=MaterialTheme.typography.titleSmall,color=MaterialTheme.colorScheme.error)
                            Text("Check the spelling, or give the full path to the program.",style=MaterialTheme.typography.bodySmall)
                        } else {
                            Text("Found",style=MaterialTheme.typography.titleSmall)
                            result.executable?.let {FolderPathText(it)}
                            Text(when {result.versionOk && result.version!=null->"Version: ${result.version}";result.versionOk->"Ran --version successfully.";else->"It exists, but --version did not succeed. Some agents do not support it; you can still save."},style=MaterialTheme.typography.bodySmall)
                        }
                    }
                }
            }
            error?.let {Text(it,color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodySmall)}
        }
        Column(Modifier.padding(horizontal=24.dp,vertical=12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            OutlinedButton({run({check=client.check(command.trim(),env)},{checking=it})},Modifier.fillMaxWidth().heightIn(min=52.dp).testTag("agent-check"),enabled=ready && !checking && !saving) {Text(if(checking)"Checking…" else "Check it is installed")}
            Button({run({client.add(AgentRegistration(id,name.trim(),command.trim(),args.lines().map {it.trim()}.filter {it.isNotEmpty()},env,ownTools));onSaved()},{saving=it})},Modifier.fillMaxWidth().heightIn(min=52.dp).testTag("agent-save"),enabled=ready && check?.found==true && !checking && !saving) {Text(if(saving)"Saving…" else "Save agent")}
        }
    }
}
