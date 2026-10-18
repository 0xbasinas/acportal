package dev.acportal.presentation

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Check
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import kotlinx.coroutines.launch

internal val GIT_TABS = listOf("Changes","History","Branches","Worktrees","Stash")

/** The Git screen's pages. The selected page is announced as selected to accessibility services. */
@Composable internal fun GitTabs(selected:Int,onSelect:(Int)->Unit) {
    ScrollableTabRow(selected,Modifier.fillMaxWidth().testTag("git-tabs"),edgePadding=16.dp,containerColor=Color.Transparent,divider={}) {
        GIT_TABS.forEachIndexed {index,title->
            Tab(selected==index,{onSelect(index)},Modifier.heightIn(min=48.dp).testTag("git-tab-$title").semantics {this.selected=selected==index;role=Role.Tab}) {
                Text(title,Modifier.padding(horizontal=8.dp,vertical=12.dp),style=MaterialTheme.typography.labelLarge,color=if(selected==index)MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
    }
}

/** Runs one git read and keeps its text, error and a way to read it again. */
@Composable private fun rememberGitRead(run:GitRun,op:String):Triple<String?,String?,()->Unit> {
    var text by remember {mutableStateOf<String?>(null)}
    var error by remember {mutableStateOf<String?>(null)}
    var generation by remember {mutableIntStateOf(0)}
    LaunchedEffect(generation) {
        run(op,emptyMap()).fold({text=it;error=null},{error=it.message ?: "The host could not read this folder."})
    }
    return Triple(text,error) {generation++}
}

private val LANE_COLORS=listOf(Color(0xFF72D5B0),Color(0xFF6CA8F0),Color(0xFFE0A64A),Color(0xFFB98AE8),Color(0xFFEF7A85),Color(0xFF8FD0E0))

/** The commits of every branch with their lines drawn between them, newest first. */
@Composable internal fun GitHistoryTab(run:GitRun,modifier:Modifier=Modifier) {
    val (text,error,reload)=rememberGitRead(run,"log-dag")
    val rows=remember(text) {text?.let {layoutGraph(parseLogDag(it))}.orEmpty()}
    val lanes=(rows.maxOfOrNull {row->maxOf(row.node,row.through.maxOrNull() ?: 0,row.joining.maxOrNull() ?: 0,row.parentLanes.maxOrNull() ?: 0)} ?: 0).coerceAtMost(5)+1
    Column(modifier.fillMaxWidth()) {
        when {
            error!=null -> Column(Modifier.padding(24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {Text(error,color=MaterialTheme.colorScheme.error);Button(reload) {Text("Try again")}}
            text==null -> Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()}
            rows.isEmpty() -> EmptyState("No commits yet","Make a commit and it appears here.")
            else -> LazyColumn(Modifier.testTag("git-history")) {
                items(rows,key={it.commit.hash}) {row->
                    Row(Modifier.fillMaxWidth().heightIn(min=56.dp).padding(start=12.dp,end=16.dp),verticalAlignment=Alignment.CenterVertically) {
                        GraphLanes(row,lanes,Modifier.width((14*lanes+8).dp).height(56.dp))
                        Column(Modifier.weight(1f).padding(start=8.dp)) {
                            Text(row.commit.subject,style=MaterialTheme.typography.bodyMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                            Text(listOf(row.commit.hash.take(7),row.commit.author,row.commit.time).joinToString(" · "),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)
                            if(row.commit.refs.isNotBlank())Text(row.commit.refs,style=MaterialTheme.typography.labelSmall,color=ConnectedGreen,maxLines=1,overflow=TextOverflow.Ellipsis)
                        }
                    }
                }
            }
        }
    }
}

@Composable private fun GraphLanes(row:GraphRow,lanes:Int,modifier:Modifier) {
    Canvas(modifier) {
        val step=14.dp.toPx()
        fun x(lane:Int)=6.dp.toPx()+lane.coerceAtMost(lanes-1)*step
        fun color(lane:Int)=LANE_COLORS[lane%LANE_COLORS.size]
        val mid=size.height/2
        val stroke=2.dp.toPx()
        row.through.forEach {lane->drawLine(color(lane),Offset(x(lane),0f),Offset(x(lane),size.height),stroke)}
        row.joining.forEach {lane->drawLine(color(lane),Offset(x(lane),0f),Offset(x(row.node),mid),stroke,StrokeCap.Round)}
        row.parentLanes.forEach {lane->drawLine(color(lane),Offset(x(row.node),mid),Offset(x(lane),size.height),stroke,StrokeCap.Round)}
        drawCircle(color(row.node),5.dp.toPx(),Offset(x(row.node),mid))
    }
}

/** Local branches to switch to, a way to start a new one, and the remote branches for reference. */
@Composable internal fun GitBranchesTab(run:GitRun,onChanged:()->Unit,modifier:Modifier=Modifier) {
    val (text,error,reload)=rememberGitRead(run,"branch-list")
    val branches=remember(text) {text?.let(::parseBranches).orEmpty()}
    val scope=rememberCoroutineScope()
    var busy by remember {mutableStateOf(false)}
    var message by remember {mutableStateOf<String?>(null)}
    var switchTo by remember {mutableStateOf<String?>(null)}
    var mergeIn by rememberSaveable {mutableStateOf<String?>(null)}
    var removing by rememberSaveable {mutableStateOf<String?>(null)}
    var creating by rememberSaveable {mutableStateOf(false)}
    var newName by rememberSaveable {mutableStateOf("")}
    fun change(op:String,params:Map<String,String>,done:String) {
        busy=true;message=null
        scope.launch {
            run(op,params).fold({message=done},{message=it.message ?: "Git refused that."})
            reload();onChanged();busy=false
        }
    }
    Column(modifier.fillMaxWidth()) {
        when {
            error!=null -> Column(Modifier.padding(24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {Text(error,color=MaterialTheme.colorScheme.error);Button(reload) {Text("Try again")}}
            text==null -> Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()}
            else -> LazyColumn(Modifier.testTag("git-branches"),contentPadding=PaddingValues(horizontal=16.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                item(key="new") {OutlinedButton({creating=true},Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("new-branch"),enabled=!busy) {Text("New branch from the current one…")}}
                message?.let {item(key="message") {Text(it,Modifier.testTag("git-message-line"),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
                items(branches.filterNot {it.remote},key={"b:${it.name}"}) {branch->
                    Row(Modifier.fillMaxWidth().heightIn(min=56.dp).clickable(enabled=!branch.current && !busy) {switchTo=branch.name}.testTag("branch-${branch.name}"),verticalAlignment=Alignment.CenterVertically) {
                        Icon(Icons.Outlined.Check,if(branch.current)"Current branch" else null,Modifier.size(18.dp),tint=if(branch.current)ConnectedGreen else Color.Transparent)
                        Column(Modifier.weight(1f).padding(start=12.dp)) {
                            Text(branch.name,style=MaterialTheme.typography.bodyLarge,maxLines=1,overflow=TextOverflow.Ellipsis)
                            Text(listOf(branch.head,branch.track.takeIf {it.isNotBlank()},branch.updated).filterNotNull().joinToString(" · "),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1)
                        }
                        if(!branch.current)TextButton({mergeIn=branch.name},enabled=!busy,modifier=Modifier.heightIn(min=48.dp).testTag("merge-${branch.name}")) {Text("Merge")}
                        if(!branch.current)TextButton({removing=branch.name},enabled=!busy,modifier=Modifier.heightIn(min=48.dp)) {Text("Delete",color=MaterialTheme.colorScheme.error)}
                    }
                }
                val remotes=branches.filter {it.remote}
                if(remotes.isNotEmpty()) {
                    item(key="remote-head") {Text("On the remote",Modifier.padding(top=12.dp),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                    items(remotes,key={"r:${it.name}"}) {Text(it.name,Modifier.padding(vertical=8.dp),style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)}
                }
            }
        }
    }
    switchTo?.let {name->AlertDialog(onDismissRequest={switchTo=null},title={Text("Switch to $name?")},text={Text("The folder's files change to that branch. Git refuses if uncommitted changes would be lost.")},confirmButton={TextButton({switchTo=null;change("checkout",mapOf("branch" to name),"Switched to $name")}) {Text("Switch")}},dismissButton={TextButton({switchTo=null}) {Text("Cancel")}})}
    mergeIn?.let {name->AlertDialog(onDismissRequest={mergeIn=null},title={Text("Merge $name into ${branches.firstOrNull {it.current}?.name ?: "this branch"}?")},text={Text("Git joins the two with a merge commit. If both changed the same lines, the merge stops and the Changes page helps you settle each file; you can abort it.")},confirmButton={TextButton({mergeIn=null;change("merge",mapOf("branch" to name),"Merged $name")}) {Text("Merge")}},dismissButton={TextButton({mergeIn=null}) {Text("Cancel")}})}
    removing?.let {name->AlertDialog(onDismissRequest={removing=null},title={Text("Delete $name?")},text={Text("Git deletes the branch only if it is merged. Nothing else is touched.")},confirmButton={TextButton({removing=null;change("branch-delete",mapOf("branch" to name),"Deleted $name")}) {Text("Delete",color=MaterialTheme.colorScheme.error)}},dismissButton={TextButton({removing=null}) {Text("Cancel")}})}
    if(creating)AlertDialog(onDismissRequest={creating=false},title={Text("New branch")},text={
        OutlinedTextField(newName,{newName=it.take(200).filterNot {c->c.isWhitespace()}},Modifier.fillMaxWidth().testTag("new-branch-name"),label={Text("Branch name")},singleLine=true)
    },confirmButton={TextButton({val name=newName;creating=false;newName="";if(name.isNotBlank())change("branch-create",mapOf("branch" to name),"Created and switched to $name")},enabled=newName.isNotBlank()) {Text("Create")}},dismissButton={TextButton({creating=false}) {Text("Cancel")}})
}

/** The folder's worktrees: other checkouts of the same repository, each on its own branch. */
@Composable internal fun GitWorktreesTab(run:GitRun,folder:String,modifier:Modifier=Modifier) {
    val (text,error,reload)=rememberGitRead(run,"worktree-list")
    val trees=remember(text) {text?.let(::parseWorktrees).orEmpty()}
    val scope=rememberCoroutineScope()
    var busy by remember {mutableStateOf(false)}
    var message by remember {mutableStateOf<String?>(null)}
    var removing by rememberSaveable {mutableStateOf<String?>(null)}
    var creating by rememberSaveable {mutableStateOf(false)}
    var branch by rememberSaveable {mutableStateOf("")}
    fun change(op:String,params:Map<String,String>,done:String) {
        busy=true;message=null
        scope.launch {
            run(op,params).fold({message=done},{message=it.message ?: "Git refused that."})
            reload();busy=false
        }
    }
    Column(modifier.fillMaxWidth()) {
        when {
            error!=null -> Column(Modifier.padding(24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {Text(error,color=MaterialTheme.colorScheme.error);Button(reload) {Text("Try again")}}
            text==null -> Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()}
            else -> LazyColumn(Modifier.testTag("git-worktrees"),contentPadding=PaddingValues(horizontal=16.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                item(key="new") {OutlinedButton({creating=true},Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("new-worktree"),enabled=!busy) {Text("New worktree on a new branch…")}}
                message?.let {item(key="message") {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
                items(trees.withIndex().toList(),key={it.value.path}) {(index,tree)->
                    Surface(Modifier.fillMaxWidth().testTag("worktree-$index"),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
                        Row(Modifier.padding(start=16.dp,top=8.dp,bottom=8.dp,end=8.dp),verticalAlignment=Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {
                                Text(tree.branch ?: if(tree.detached)"detached at ${tree.head}" else tree.head,style=MaterialTheme.typography.bodyLarge,maxLines=1,overflow=TextOverflow.Ellipsis)
                                Text(tree.path,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=2,overflow=TextOverflow.Ellipsis)
                                if(index==0)Text("Main folder",style=MaterialTheme.typography.labelSmall,color=ConnectedGreen)
                            }
                            if(index>0)TextButton({removing=tree.path},enabled=!busy,modifier=Modifier.heightIn(min=48.dp)) {Text("Remove",color=MaterialTheme.colorScheme.error)}
                        }
                    }
                }
            }
        }
    }
    removing?.let {path->AlertDialog(onDismissRequest={removing=null},title={Text("Remove this worktree?")},text={Text("$path is removed from disk. Git refuses if it has uncommitted changes. The branch stays.")},confirmButton={TextButton({removing=null;change("worktree-remove",mapOf("path" to path),"Removed the worktree")}) {Text("Remove",color=MaterialTheme.colorScheme.error)}},dismissButton={TextButton({removing=null}) {Text("Cancel")}})}
    if(creating) {
        val parent=plainPath(folder).trimEnd('/','\\').let {it.substringBeforeLast('/',it.substringBeforeLast('\\',""))}
        val separator=if(plainPath(folder).contains('\\'))"\\" else "/"
        val target=if(parent.isBlank() || branch.isBlank()) "" else parent+separator+folderName(folder)+"-"+branch.replace(Regex("[^A-Za-z0-9_.-]"),"-")
        AlertDialog(onDismissRequest={creating=false},title={Text("New worktree")},text={
            Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(branch,{branch=it.take(200).filterNot {c->c.isWhitespace()}},Modifier.fillMaxWidth().testTag("new-worktree-branch"),label={Text("New branch name")},singleLine=true)
                if(target.isNotBlank())Text("Made at $target, next to this folder. The host must allow that location.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            }
        },confirmButton={TextButton({val name=branch;creating=false;branch="";if(name.isNotBlank() && target.isNotBlank())change("worktree-add",mapOf("branch" to name,"path" to target),"Made a worktree on $name")},enabled=target.isNotBlank()) {Text("Create")}},dismissButton={TextButton({creating=false}) {Text("Cancel")}})
    }
}

/** Unfinished work set aside: put away every change, then bring it back (apply keeps it, pop removes it) or drop it. */
@Composable internal fun GitStashTab(run:GitRun,hasChanges:Boolean,onChanged:()->Unit,modifier:Modifier=Modifier) {
    val (text,error,reload)=rememberGitRead(run,"stash-list")
    val stashes=remember(text) {text?.let(::parseStashes).orEmpty()}
    val scope=rememberCoroutineScope()
    var busy by remember {mutableStateOf(false)}
    var message by remember {mutableStateOf<String?>(null)}
    var dropping by rememberSaveable {mutableStateOf<String?>(null)}
    var naming by rememberSaveable {mutableStateOf(false)}
    var label by rememberSaveable {mutableStateOf("")}
    fun change(op:String,params:Map<String,String>,done:String) {
        busy=true;message=null
        scope.launch {
            run(op,params).fold({message=done},{message=it.message ?: "Git refused that."})
            reload();onChanged();busy=false
        }
    }
    Column(modifier.fillMaxWidth()) {
        when {
            error!=null -> Column(Modifier.padding(24.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {Text(error,color=MaterialTheme.colorScheme.error);Button(reload) {Text("Try again")}}
            text==null -> Box(Modifier.fillMaxSize(),contentAlignment=Alignment.Center) {CircularProgressIndicator()}
            else -> LazyColumn(Modifier.testTag("git-stash"),contentPadding=PaddingValues(horizontal=16.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                item(key="intro") {Text("The stash is a shelf for unfinished work: it cleans the folder without committing, and you can bring the changes back later.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                item(key="push") {Button({naming=true},Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("stash-push"),enabled=hasChanges && !busy) {Text("Stash all changes…")}}
                message?.let {item(key="message") {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}}
                if(stashes.isEmpty())item(key="empty") {Text("The stash is empty.",Modifier.padding(vertical=8.dp),style=MaterialTheme.typography.bodyMedium)}
                items(stashes,key={it.ref}) {stash->
                    Surface(Modifier.fillMaxWidth().testTag("stash-${stash.ref}"),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
                        Column(Modifier.padding(start=16.dp,top=8.dp,end=8.dp,bottom=4.dp)) {
                            Text(stash.message.ifBlank {stash.ref},style=MaterialTheme.typography.bodyMedium,maxLines=2,overflow=TextOverflow.Ellipsis)
                            Text("${stash.ref} · ${stash.time}",style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                            Row(horizontalArrangement=Arrangement.spacedBy(4.dp)) {
                                TextButton({change("stash-apply",mapOf("ref" to stash.ref),"Applied ${stash.ref}")},enabled=!busy,modifier=Modifier.heightIn(min=48.dp)) {Text("Apply")}
                                TextButton({change("stash-pop",mapOf("ref" to stash.ref),"Popped ${stash.ref}")},enabled=!busy,modifier=Modifier.heightIn(min=48.dp).testTag("pop-${stash.ref}")) {Text("Pop")}
                                TextButton({dropping=stash.ref},enabled=!busy,modifier=Modifier.heightIn(min=48.dp)) {Text("Drop",color=MaterialTheme.colorScheme.error)}
                            }
                        }
                    }
                }
            }
        }
    }
    dropping?.let {ref->AlertDialog(onDismissRequest={dropping=null},title={Text("Delete $ref?")},text={Text("Its changes are gone for good; the app cannot bring them back.")},confirmButton={TextButton({dropping=null;change("stash-drop",mapOf("ref" to ref),"Dropped $ref")}) {Text("Delete",color=MaterialTheme.colorScheme.error)}},dismissButton={TextButton({dropping=null}) {Text("Cancel")}})}
    if(naming)AlertDialog(onDismissRequest={naming=false},title={Text("Stash all changes")},text={
        OutlinedTextField(label,{label=it.take(200)},Modifier.fillMaxWidth().testTag("stash-label"),label={Text("Label (optional)")},singleLine=true)
    },confirmButton={TextButton({val text=label.trim();naming=false;label="";change("stash-push",if(text.isEmpty())emptyMap() else mapOf("message" to text),"Stashed the changes")}) {Text("Stash")}},dismissButton={TextButton({naming=false}) {Text("Cancel")}})
}
