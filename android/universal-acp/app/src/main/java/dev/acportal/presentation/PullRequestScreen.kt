package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import dev.acportal.data.PullRequestRequest
import dev.acportal.protocol.isBaseBranch
import kotlinx.coroutines.launch

/**
 * Opens a pull request with the host's `gh`. The title and description are typed here; nothing is
 * suggested and nothing is sent until the button is pressed. A fork owner turns the head into
 * `owner:branch`. The address of the new (or already existing) pull request is shown as a button
 * and is never opened by itself.
 */
@Composable fun PullRequestScreen(hostLabel:String?,workspace:String,loadBranch:suspend ()->String?,onSubmit:suspend (PullRequestRequest)->Result<String>,onBack:()->Unit,suggestions:GitSuggestions?=null) {
    var base by rememberSaveable {mutableStateOf("main")}
    var branch by rememberSaveable {mutableStateOf("")}
    var branchLoaded by rememberSaveable {mutableStateOf(false)}
    var owner by rememberSaveable {mutableStateOf("")}
    var title by rememberSaveable {mutableStateOf("")}
    var body by rememberSaveable {mutableStateOf("")}
    var draft by rememberSaveable {mutableStateOf(false)}
    var busy by remember {mutableStateOf(false)}
    var suggesting by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    var url by rememberSaveable {mutableStateOf<String?>(null)}
    val scope=rememberCoroutineScope()
    val uriHandler=LocalUriHandler.current
    LaunchedEffect(workspace) {
        if(!branchLoaded) {
            loadBranch()?.let {if(branch.isBlank())branch=it}
            branchLoaded=true
        }
    }
    // On the base branch there is nothing to compare, so no form is offered (the desktop does the same).
    val onBase=branchLoaded && isBaseBranch(base,branch.takeIf {it.isNotBlank()})
    val ready=!busy && title.isNotBlank() && base.isNotBlank() && branch.isNotBlank()
    val folder=workspace.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\')
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Open pull request",subtitle=hostLabel,onBack=onBack)
        if(onBase)Column(Modifier.fillMaxSize().padding(24.dp).testTag("pr-on-base"),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Text("You are on $branch",style=MaterialTheme.typography.titleMedium)
            Text("$branch is the base branch, so there is nothing to compare. Switch the folder to a feature branch on the host, then open a pull request from it.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedButton(onBack,Modifier.heightIn(min=48.dp)) {Text("Back")}
        } else {
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal=24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Text("Runs gh on the host for $folder. Push the branch first; the host's own git and GitHub sign-in are used.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            OutlinedTextField(branch,{branch=it.take(255)},Modifier.fillMaxWidth().testTag("pr-branch"),label={Text("Branch to merge")},singleLine=true,enabled=!busy,supportingText={if(branchLoaded && branch.isBlank())Text("The host reported no checked-out branch.")})
            OutlinedTextField(base,{base=it.take(255)},Modifier.fillMaxWidth().testTag("pr-base"),label={Text("Into")},singleLine=true,enabled=!busy)
            OutlinedTextField(owner,{owner=it.take(39)},Modifier.fillMaxWidth().testTag("pr-owner"),label={Text("Fork owner (only for a fork)")},singleLine=true,enabled=!busy)
            if(suggestions!=null && branch.isNotBlank() && base.isNotBlank())SuggestButton(suggestions.agents,suggesting,"pr-suggest",Modifier.fillMaxWidth()) {agent->
                suggesting=true;error=null
                scope.launch {
                    suggestions.pullRequest(agent.id,base.trim()).onSuccess {(suggestedTitle,suggestedBody)->title=suggestedTitle.take(256);body=suggestedBody.take(60_000)}.onFailure {error=it.message ?: "The agent could not write it."}
                    suggesting=false
                }
            }
            OutlinedTextField(title,{title=it.take(256)},Modifier.fillMaxWidth().testTag("pr-title"),label={Text("Title")},singleLine=true,enabled=!busy)
            OutlinedTextField(body,{body=it.take(60_000)},Modifier.fillMaxWidth().heightIn(min=140.dp).testTag("pr-body"),label={Text("What changed and why")},enabled=!busy)
            Row(Modifier.fillMaxWidth().heightIn(min=48.dp).toggleable(draft,enabled=!busy,role=Role.Switch,onValueChange={draft=it}),verticalAlignment=Alignment.CenterVertically) {
                Text("Draft",Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
                Switch(draft,null,enabled=!busy)
            }
            error?.let {Text(it,Modifier.testTag("pr-error"),color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodyMedium)}
            url?.takeIf {it.startsWith("https://")}?.let {link->
                Text("Pull request",style=MaterialTheme.typography.titleSmall)
                Text(link,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                OutlinedButton({uriHandler.openUri(link)},Modifier.heightIn(min=48.dp).testTag("pr-open-link")) {Text("View pull request")}
            }
        }
        Button({
            val request=PullRequestRequest(workspace,base.trim(),branch.trim(),owner.trim(),title.trim(),body,draft)
            busy=true;error=null;url=null
            scope.launch {
                onSubmit(request).onSuccess {url=it}.onFailure {error=it.message ?: "The pull request was not opened."}
                busy=false
            }
        },Modifier.fillMaxWidth().padding(24.dp).heightIn(min=52.dp).testTag("pr-submit"),enabled=ready) {Text(if(busy)"Opening…" else "Open pull request")}
        }
    }
}
