package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import dev.acportal.protocol.*

/** One line under a task row: state, agent rights, delegation tools and waiting requests. */
internal fun taskRowSupporting(task:TaskSummary):String = listOfNotNull(
    taskStateLabel(task.state),
    task.pendingDecisions.takeIf {it>0}?.let {if(it==1)"1 request waiting" else "$it requests waiting"},
    delegateLabel(task.delegate),
    task.summary,
).joinToString(" · ")

@Composable fun TasksScreen(state:TasksUiState,onBack:()->Unit,onRefresh:()->Unit,onTask:(String)->Unit) {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Delegated tasks","Experimental",onBack,action={IconButton(onRefresh,enabled=!state.loading) {Icon(Icons.Outlined.Refresh,"Refresh tasks")}})
        state.error?.let {Text(it,Modifier.padding(horizontal=24.dp,vertical=8.dp),color=MaterialTheme.colorScheme.error)}
        val tasks=state.tasks
        when {
            tasks==null -> EmptyState(if(state.loading)"Loading tasks" else "Tasks unavailable","Agents on this host can delegate work to other agents. Their task trees appear here.")
            tasks.isEmpty() -> EmptyState("No delegated tasks","When an agent delegates work, each task and its sub-tasks appear here.")
            else -> LazyColumn(Modifier.testTag("task-tree")) {
                items(flattenTaskTree(buildTaskTrees(tasks)),key={it.task.id}) {row->
                    val task=row.task
                    ListItem(modifier=Modifier.padding(start=(row.indent.coerceAtMost(6)*20).dp).clickable {onTask(task.id)}.semantics {contentDescription="${if(row.indent==0)"Root task" else "Sub-task level ${row.indent}"}, ${task.agent}, ${taskRowSupporting(task)}"},
                        headlineContent={Text(task.agent)},supportingContent={Text(taskRowSupporting(task))},
                        leadingContent={Icon(if(row.indent==0)Icons.Outlined.AccountTree else Icons.Outlined.SubdirectoryArrowRight,null)},
                        trailingContent={if(task.pendingDecisions>0)Badge {Text("${task.pendingDecisions}")}})
                }
            }
        }
    }
}

@Composable fun TaskDetailScreen(state:TasksUiState,taskId:String,onBack:()->Unit,onRefresh:()->Unit,onCancel:()->Unit,onSupervision:(Boolean)->Unit,onSession:((String)->Unit)?) {
    val detail=state.detail?.takeIf {it.id==taskId}
    var confirmCancel by remember {mutableStateOf(false)}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader(detail?.agent ?: "Task",detail?.let {taskStateLabel(it.state)},onBack,action={IconButton(onRefresh,enabled=!state.loading) {Icon(Icons.Outlined.Refresh,"Refresh task")}})
        state.error?.let {Text(it,Modifier.padding(horizontal=24.dp,vertical=8.dp),color=MaterialTheme.colorScheme.error)}
        if(detail==null) {EmptyState(if(state.loading)"Loading task" else "Task unavailable","Refresh to read this task from the host.");return@Column}
        LazyColumn {
            item {SectionLabel("Task")}
            item {ListItem(headlineContent={Text("Folder")},supportingContent={FolderPathText(detail.scope)})}
            item {ListItem(headlineContent={Text("Rights")},supportingContent={Text(taskRightsLabel(detail.rights))})}
            delegateLabel(detail.delegate)?.let {label->item {ListItem(headlineContent={Text("Delegation tools")},supportingContent={Text(label+if(detail.delegate=="not_connected")". The agent may not have started them, so it cannot delegate further." else "")})}}
            (detail.summary ?: detail.stopReason)?.let {why->item {ListItem(headlineContent={Text("Outcome")},supportingContent={Text(why)})}}
            detail.costUsd?.let {cost->item {ListItem(headlineContent={Text("Reported cost")},supportingContent={Text("US$"+"%.4f".format(cost))})}}
            item {SectionLabel("Waiting for a decision")}
            if(detail.needsDecision.isEmpty()) item {Text("Nothing is waiting.",Modifier.padding(horizontal=24.dp,vertical=12.dp),color=MaterialTheme.colorScheme.onSurfaceVariant)}
            items(detail.needsDecision) {decision->
                ListItem(modifier=Modifier.testTag("task-decision"),headlineContent={Text(listOfNotNull(decision.kind,decision.source).joinToString(" · ").ifEmpty {"Request"})},
                    supportingContent={Text(supervisorOutcomeLabel(decision)+(decision.expiresInSeconds?.let {" · supervisor window ${it}s"} ?: ""),color=decisionColor(decision.decidable))},
                    leadingContent={Icon(if(decision.decidable)Icons.Outlined.SmartToy else Icons.Outlined.PanTool,null)})
            }
            if(detail.needsDecision.isNotEmpty()) item {Text("You can answer any of these in the task's session; your answer always wins over the supervisor's.",Modifier.padding(horizontal=24.dp,vertical=8.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
            item {SectionLabel("Actions")}
            val session=detail.session
            if(session!=null && onSession!=null) item {TextButton({onSession(session)},Modifier.padding(horizontal=12.dp)) {Text("Open session")}}
            if(detail.parent==null) item {Row(Modifier.padding(horizontal=12.dp)) {
                TextButton({onSupervision(true)},enabled=!state.loading) {Text("Pause supervisor")}
                TextButton({onSupervision(false)},enabled=!state.loading) {Text("Resume supervisor")}
            }}
            if(taskIsOpen(detail.state) || detail.parent==null) item {TextButton({confirmCancel=true},Modifier.padding(horizontal=12.dp),enabled=!state.loading) {Text(if(detail.parent==null)"Cancel delegated work" else "Cancel task",color=MaterialTheme.colorScheme.error)}}
        }
    }
    if(confirmCancel) AlertDialog(onDismissRequest={confirmCancel=false},title={Text("Cancel this task?")},text={Text("Everything still running below it is cancelled too. Nothing is restarted.")},confirmButton={TextButton({confirmCancel=false;onCancel()}) {Text("Cancel task")}},dismissButton={TextButton({confirmCancel=false}) {Text("Keep running")}})
}

/** Matches Penpot boards 52: green when the supervisor may decide, amber when only you can. */
@Composable private fun decisionColor(decidable:Boolean):Color {
    val dark=MaterialTheme.colorScheme.background.luminance()<0.5f
    return if(decidable) {if(dark)Color(0xFF72C7A2) else Color(0xFF2F8A63)} else {if(dark)Color(0xFFE5A85A) else Color(0xFFA66510)}
}
