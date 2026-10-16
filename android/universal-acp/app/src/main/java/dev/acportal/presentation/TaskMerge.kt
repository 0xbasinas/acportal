package dev.acportal.presentation

import dev.acportal.protocol.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/**
 * Do something to a task's work that cannot be undone (merge it, throw it away), say so, then
 * refresh. The notice is set before the refresh, so a refresh that fails is reported next to it
 * instead of replacing it, and a failed action changes nothing. Cancellation is never turned into
 * an error.
 */
private suspend fun <T> actThenRefresh(state:MutableStateFlow<TasksUiState>,act:suspend ()->T,notice:(T)->String,refreshFailed:String,list:suspend ()->List<TaskSummary>,detail:suspend ()->TaskDetail) {
    val done=act()
    state.update {it.copy(notice=notice(done))}
    try {
        val tasks=list();val task=detail()
        state.update {it.copy(tasks=tasks,detail=task)}
    } catch(cancelled:CancellationException) {throw cancelled}
    catch(failure:Exception) {state.update {it.copy(error=refreshFailed)}}
}

internal suspend fun mergeAndRefresh(state:MutableStateFlow<TasksUiState>,merge:suspend ()->TaskMergeResult,list:suspend ()->List<TaskSummary>,detail:suspend ()->TaskDetail) =
    actThenRefresh(state,merge,{"Merged ${it.branch} into the parent's folder (${it.merged.take(7)})."},"The merge is done, but the task list could not be refreshed. Refresh to see it.",list,detail)

internal suspend fun discardAndRefresh(state:MutableStateFlow<TasksUiState>,discard:suspend ()->TaskDiscardResult,list:suspend ()->List<TaskSummary>,detail:suspend ()->TaskDetail) =
    actThenRefresh(state,discard,{"Threw away the work of ${it.discarded}."},"The work was thrown away, but the task list could not be refreshed. Refresh to see it.",list,detail)
