package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import dev.acportal.protocol.*
import kotlinx.coroutines.launch

/** What the host's git said, or why it could not run. The text of a failed command is shown as is. */
fun interface GitRun {
    /** [input] is a patch for the apply operations; every other operation takes none. */
    suspend fun run(op:String,params:Map<String,String>,input:String?):Result<String>
}
suspend operator fun GitRun.invoke(op:String,params:Map<String,String>):Result<String> = run(op,params,null)

/** Lines of one diff drawn before the rest is summarised, so a huge file stays light. */
internal const val GIT_LINES_SHOWN = 200

private sealed interface NewFile {
    data object Loading : NewFile
    data class Done(val diff:FileDiffText?) : NewFile
    data class Failed(val message:String) : NewFile
}

/**
 * Git for one folder, through the host's fixed git operations: the files changed and the files
 * staged, with several files staged or unstaged in one step, new files readable, and every file
 * foldable or all of them at once. Nothing is committed or staged until a button is pressed.
 */
@Composable fun GitScreen(folder:String,hostLabel:String?,run:GitRun,onPullRequest:(()->Unit)?,onBack:()->Unit) {
    var status by remember {mutableStateOf<GitStatus?>(null)}
    var unstagedDiffs by remember {mutableStateOf<Map<String,FileDiffText>>(emptyMap())}
    var stagedDiffs by remember {mutableStateOf<Map<String,FileDiffText>>(emptyMap())}
    var newFiles by remember {mutableStateOf<Map<String,NewFile>>(emptyMap())}
    var loading by remember {mutableStateOf(true)}
    var busy by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    var notice by remember {mutableStateOf<String?>(null)}
    // "u:<path>" or "s:<path>"; saved as lists so they survive rotation.
    var picked by rememberSaveable {mutableStateOf(emptyList<String>())}
    var open by rememberSaveable {mutableStateOf(emptyList<String>())}
    var committing by rememberSaveable {mutableStateOf(false)}
    var tab by rememberSaveable {mutableIntStateOf(0)}
    // Bumped by Refresh so the History, Branches and Worktrees pages read again too.
    var version by remember {mutableIntStateOf(0)}
    var message by rememberSaveable {mutableStateOf("")}
    val scope=rememberCoroutineScope()

    suspend fun load() {
        loading=true
        val statusText=run("status",emptyMap())
        statusText.onFailure {error=it.message ?: "The host could not read this folder's git status.";loading=false;return}
        val parsed=parseGitStatus(statusText.getOrThrow())
        status=parsed
        unstagedDiffs=run("diff",emptyMap()).getOrNull()?.let(::parseUnifiedDiff)?.associateBy {it.path}.orEmpty()
        stagedDiffs=run("diff",mapOf("staged" to "1")).getOrNull()?.let(::parseUnifiedDiff)?.associateBy {it.path}.orEmpty()
        // What was ticked or opened for a file that is no longer changed goes away; new files are read again.
        val now=parsed.unstaged.map {"u:${it.path}"}+parsed.staged.map {"s:${it.path}"}
        picked=picked.filter {it in now}
        open=open.filter {it in now}
        newFiles=emptyMap()
        error=null
        loading=false
    }
    LaunchedEffect(Unit) {load()}
    // A new file has no diff until it is staged; read it when it is opened.
    LaunchedEffect(open,status) {
        val wanted=status?.unstaged.orEmpty().filter {it.untracked && "u:${it.path}" in open && it.path !in newFiles}.map {it.path}
        if(wanted.isEmpty())return@LaunchedEffect
        newFiles=newFiles+wanted.associateWith {NewFile.Loading}
        for(path in wanted) {
            val read=run("diff-new",mapOf("file" to path))
            newFiles=newFiles+(path to read.fold({NewFile.Done(parseUnifiedDiff(it).firstOrNull())},{failure->NewFile.Failed(if(failure.message.orEmpty().contains("older than the app"))"This host is older than the app and cannot show a new file yet. Update and restart acpd." else "Could not read this file: ${failure.message}")}))
        }
    }

    fun change(label:String,ops:List<Pair<String,String>>) {
        if(busy||ops.isEmpty())return
        busy=true;notice=null
        scope.launch {
            var done=0
            var failure:String?=null
            for((op,file) in ops) {
                run(op,mapOf("file" to file)).onFailure {failure=it.message ?: "Git refused that change.";return@onFailure}
                if(failure!=null)break
                done++
            }
            load()
            if(failure!=null)error="$failure ($done of ${ops.size} files $label before it stopped.)" else notice="${if(ops.size==1)"1 file" else "${ops.size} files"} $label"
            picked=emptyList()
            busy=false
        }
    }

    fun hunk(diffs:Map<String,FileDiffText>,path:String,index:Int,stage:Boolean) {
        val patch=diffs[path]?.let {hunkPatch(it,index)}
        if(busy)return
        if(patch==null) {error="This change cannot be staged by hunk. Stage the whole file instead.";return}
        busy=true;notice=null
        scope.launch {
            run.run(if(stage)"apply-cached" else "apply-cached-reverse",emptyMap(),patch).fold({notice=if(stage)"Staged one change of $path" else "Unstaged one change of $path"},{error=it.message ?: "Git refused that change."})
            load();busy=false
        }
    }
    val current=status
    val unstaged=current?.unstaged.orEmpty()
    val staged=current?.staged.orEmpty()
    val tickedUnstaged=unstaged.filter {"u:${it.path}" in picked}
    val tickedStaged=staged.filter {"s:${it.path}" in picked}
    val allKeys=unstaged.map {"u:${it.path}"}+staged.map {"s:${it.path}"}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Git · ${folder.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\')}",subtitle=listOfNotNull(hostLabel,current?.branch?.let {"on $it"} ?: if(current!=null)"detached head" else null,current?.let {"${it.files.size} changed"}).joinToString(" · ").ifBlank {null},onBack=onBack,action={
            IconButton({version++;scope.launch {load()}},enabled=!loading && !busy) {Icon(Icons.Outlined.Refresh,"Refresh changes",Modifier.size(20.dp))}
        })
        GitTabs(tab) {tab=it}
        if(tab==0) {
        if(loading && current==null)Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()}
        else if(current==null)Column(Modifier.padding(24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
            Text(error ?: "Git could not be read for this folder.",color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodyMedium)
            Button({scope.launch {load()}}) {Text("Try again")}
        } else LazyColumn(Modifier.weight(1f).testTag("git-list"),contentPadding=PaddingValues(horizontal=16.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            error?.let {item(key="error") {Text(it,Modifier.testTag("git-error"),color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodyMedium)}}
            notice?.let {item(key="notice") {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
            if(current.files.isEmpty())item(key="clean") {EmptyState("Nothing changed","This folder matches its last commit.")}
            if(allKeys.isNotEmpty())item(key="fold-bar") {Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                TextButton({open=allKeys},Modifier.heightIn(min=48.dp).testTag("expand-all")) {Text("Expand all")}
                TextButton({open=emptyList()},Modifier.heightIn(min=48.dp).testTag("collapse-all")) {Text("Collapse all")}
            }}
            if(unstaged.isNotEmpty()) {
                item(key="unstaged-head") {SectionHead("Unstaged · ${unstaged.size}",unstaged.map {"u:${it.path}"},picked,{picked=it},busy,if(tickedUnstaged.isNotEmpty())"Stage selected (${tickedUnstaged.size})" else null,{change("staged",tickedUnstaged.map {"add" to it.path})},"stage-selected")}
                items(unstaged,key={"u:${it.path}"}) {file->FileCard(file,"u:${file.path}",unstagedDiffs[file.path],newFiles[file.path],"u:${file.path}" in picked,"u:${file.path}" in open,{on->picked=if(on)picked+"u:${file.path}" else picked-"u:${file.path}"},{on->open=if(on)open+"u:${file.path}" else open-"u:${file.path}"},"Stage",!busy,{index->hunk(unstagedDiffs,file.path,index,true)}) {change("staged",listOf("add" to file.path))}}
            }
            if(staged.isNotEmpty()) {
                item(key="staged-head") {SectionHead("Staged · ${staged.size}",staged.map {"s:${it.path}"},picked,{picked=it},busy,if(tickedStaged.isNotEmpty())"Unstage selected (${tickedStaged.size})" else null,{change("unstaged",tickedStaged.map {"restore-staged" to it.path})},"unstage-selected")}
                items(staged,key={"s:${it.path}"}) {file->FileCard(file,"s:${file.path}",stagedDiffs[file.path],null,"s:${file.path}" in picked,"s:${file.path}" in open,{on->picked=if(on)picked+"s:${file.path}" else picked-"s:${file.path}"},{on->open=if(on)open+"s:${file.path}" else open-"s:${file.path}"},"Unstage",!busy,{index->hunk(stagedDiffs,file.path,index,false)}) {change("unstaged",listOf("restore-staged" to file.path))}}
                item(key="commit") {Button({committing=true},Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("git-commit"),enabled=!busy) {Text("Commit ${staged.size} ${if(staged.size==1)"file" else "files"}…")}}
            }
            if(onPullRequest!=null && current.branch!=null)item(key="pull-request") {
                if(isBaseBranch("main",current.branch) || isBaseBranch("master",current.branch))Text("You are on ${current.branch}, the base branch. Switch to a feature branch on the host to open a pull request.",Modifier.testTag("git-base-branch"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                else OutlinedButton(onPullRequest,Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("git-open-pr")) {Text("Open pull request from ${current.branch}")}
            }
        }
        } else Box(Modifier.weight(1f).fillMaxWidth()) {
            key(version,tab) {
                when(tab) {
                    1->GitHistoryTab(run,Modifier.fillMaxSize())
                    2->GitBranchesTab(run,{scope.launch {load()}},Modifier.fillMaxSize())
                    else->GitWorktreesTab(run,folder,Modifier.fillMaxSize())
                }
            }
        }
    }
    if(committing)AlertDialog(onDismissRequest={committing=false},title={Text("Commit staged files")},text={
        OutlinedTextField(message,{message=it.take(2000)},Modifier.fillMaxWidth().testTag("git-message"),label={Text("Message")},minLines=2)
    },confirmButton={TextButton({
        val text=message.trim()
        committing=false
        if(text.isNotEmpty()) {busy=true;notice=null;scope.launch {
            run("commit",mapOf("message" to text)).onSuccess {message="";notice="Committed ${staged.size} ${if(staged.size==1)"file" else "files"}"}.onFailure {error=it.message ?: "Git refused the commit."}
            load();busy=false
        }}
    },enabled=message.isNotBlank()) {Text("Commit")}},dismissButton={TextButton({committing=false}) {Text("Cancel")}})
}

@Composable private fun SectionHead(title:String,keys:List<String>,picked:List<String>,onPicked:(List<String>)->Unit,busy:Boolean,action:String?,onAction:()->Unit,tag:String) {
    val all=keys.isNotEmpty() && keys.all {it in picked}
    Row(Modifier.fillMaxWidth().padding(top=8.dp),verticalAlignment=Alignment.CenterVertically) {
        Checkbox(all,{on->onPicked(if(on)(picked+keys).distinct() else picked-keys.toSet())},Modifier.testTag("pick-all-$tag"))
        Text(title,Modifier.weight(1f),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant)
        if(action!=null)Button(onAction,Modifier.heightIn(min=40.dp).testTag(tag),enabled=!busy,contentPadding=PaddingValues(horizontal=16.dp)) {Text(action,maxLines=1)}
    }
}

@Composable private fun FileCard(file:GitFile,key:String,diff:FileDiffText?,newFile:NewFile?,ticked:Boolean,isOpen:Boolean,onTick:(Boolean)->Unit,onOpen:(Boolean)->Unit,actionLabel:String,enabled:Boolean,onHunk:((Int)->Unit)?=null,onAction:()->Unit) {
    Surface(Modifier.fillMaxWidth().testTag("file-$key"),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
        Column {
            Row(Modifier.fillMaxWidth().clickable {onOpen(!isOpen)}.semantics {stateDescription=if(isOpen)"Expanded" else "Collapsed"}.padding(end=8.dp),verticalAlignment=Alignment.CenterVertically) {
                Checkbox(ticked,onTick,Modifier.testTag("pick-$key"))
                Column(Modifier.weight(1f)) {
                    Text(file.path,style=MaterialTheme.typography.bodyMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                    Text(file.label+(diff?.let {" · +${it.added} −${it.removed}"} ?: ""),style=MaterialTheme.typography.labelSmall,color=if(file.untracked)ConnectedGreen else MaterialTheme.colorScheme.onSurfaceVariant)
                }
                TextButton(onAction,enabled=enabled,modifier=Modifier.heightIn(min=48.dp)) {Text(actionLabel)}
                Icon(if(isOpen)Icons.Outlined.ExpandMore else Icons.Outlined.ChevronRight,if(isOpen)"Hide ${file.path}" else "Show ${file.path}",Modifier.size(20.dp),tint=MaterialTheme.colorScheme.onSurfaceVariant)
            }
            if(isOpen) {
                val shown=when {
                    file.untracked -> when(newFile) {
                        null,NewFile.Loading -> null
                        is NewFile.Failed -> {Text(newFile.message,Modifier.padding(12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error);return@Column}
                        is NewFile.Done -> newFile.diff ?: FileDiffText(file.path,0,0,emptyList())
                    }
                    else -> diff
                }
                if(shown==null)Text(if(file.untracked)"Reading the file…" else "No text changes to show (a binary file, a rename or a mode change).",Modifier.padding(12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                else if(shown.lines.isEmpty())Text(if(file.untracked)"This file is empty or not text." else "No text changes to show (a binary file, a rename or a mode change).",Modifier.padding(12.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                else Column(Modifier.fillMaxWidth().padding(bottom=8.dp)) {
                    var hunkIndex=-1
                    shown.lines.take(GIT_LINES_SHOWN).forEach {line->
                        if(line.kind=='@') {
                            hunkIndex++
                            val thisHunk=hunkIndex
                            Row(Modifier.fillMaxWidth().padding(start=12.dp,end=4.dp),verticalAlignment=Alignment.CenterVertically) {
                                Text(line.text.substringBefore(" @@")+" @@",Modifier.weight(1f),fontFamily=FontFamily.Monospace,fontSize=11.sp,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)
                                if(onHunk!=null && !file.untracked && shown.hunks.isNotEmpty())TextButton({onHunk(thisHunk)},enabled=enabled,modifier=Modifier.heightIn(min=40.dp).testTag("hunk-$key-$thisHunk")) {Text("$actionLabel change")}
                            }
                            return@forEach
                        }
                        Text(when(line.kind) {'+'->"+ ";'-'->"− ";'@'->"";else->"  "}+line.text,Modifier.fillMaxWidth().padding(horizontal=12.dp),fontFamily=FontFamily.Monospace,fontSize=12.sp,
                            color=when(line.kind) {'+'->ConnectedGreen;'-'->MaterialTheme.colorScheme.error;else->MaterialTheme.colorScheme.onSurfaceVariant},maxLines=3,overflow=TextOverflow.Ellipsis)
                    }
                    if(shown.lines.size>GIT_LINES_SHOWN)Text("… ${shown.lines.size-GIT_LINES_SHOWN} more lines",Modifier.padding(horizontal=12.dp,vertical=4.dp),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}
