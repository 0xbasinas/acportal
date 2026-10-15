package dev.acportal.presentation

import dev.acportal.protocol.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.update

/**
 * Merge a task's branch, say so, then refresh. The notice is set before the refresh, so a refresh
 * that fails (the merge is already done and cannot be undone) is reported next to it instead of
 * replacing it, and a failed merge changes nothing. Cancellation is never turned into an error.
 */
internal suspend fun mergeAndRefresh(state:MutableStateFlow<TasksUiState>,merge:suspend ()->TaskMergeResult,list:suspend ()->List<TaskSummary>,detail:suspend ()->TaskDetail) {
    val merged=merge()
    state.update {it.copy(notice="Merged ${merged.branch} into the parent's folder (${merged.merged.take(7)}).")}
    try {
        val tasks=list();val task=detail()
        state.update {it.copy(tasks=tasks,detail=task)}
    } catch(cancelled:CancellationException) {throw cancelled}
    catch(failure:Exception) {state.update {it.copy(error="The merge is done, but the task list could not be refreshed. Refresh to see it.")}}
}
