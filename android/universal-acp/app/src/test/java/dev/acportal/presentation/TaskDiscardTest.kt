package dev.acportal.presentation

import dev.acportal.protocol.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.IOException

class TaskDiscardTest {
    private val discarded=TaskDiscardResult(discarded="acpd/task-1")
    private val summary=TaskSummary(id="i",root="i",agent="w",state="completed")
    private val detail=TaskDetail(id="i",root="i",agent="w",state="completed")
    private val before=TasksUiState(hostId="h",tasks=listOf(summary),detail=null,loading=true)

    @Test fun aDiscardSaysSoAndShowsTheFreshTask() {
        val state=MutableStateFlow(before)
        runBlocking {discardAndRefresh(state,{discarded},{listOf(summary)},{detail})}
        assertEquals("Threw away the work of acpd/task-1.",state.value.notice)
        assertNull(state.value.error)
        assertEquals(detail,state.value.detail)
    }
    @Test fun aFailedRefreshAfterTheDiscardKeepsTheNoticeAndSaysItIsDone() {
        val state=MutableStateFlow(before)
        runBlocking {discardAndRefresh(state,{discarded},{throw IOException("down")},{detail})}
        assertEquals("Threw away the work of acpd/task-1.",state.value.notice)
        assertTrue(state.value.error!!.startsWith("The work was thrown away, but"))
    }
    @Test fun aRefusedDiscardChangesNothing() {
        val state=MutableStateFlow(before)
        assertThrows(IllegalStateException::class.java) {runBlocking {discardAndRefresh(state,{error("nothing left")},{listOf(summary)},{detail})}}
        assertEquals(before,state.value)
    }
    @Test fun onlyFinishedTasksWithKeptWorkOrALiveWorktreeCanBeDiscarded() {
        fun w(outcome:String)=TaskWorktree(branch="b",outcome=outcome)
        for(outcome in listOf("active","branch_kept","snapshotted","kept_dirty","cleanup_failed")) assertTrue(outcome,canDiscardWorktree("completed",w(outcome)))
        for(outcome in listOf("removed","merged","discarded","releasing")) assertFalse(outcome,canDiscardWorktree("completed",w(outcome)))
        assertFalse("a running task",canDiscardWorktree("working",w("active")))
        assertFalse("no worktree",canDiscardWorktree("completed",null))
        assertEquals("b · Discarded; its work was thrown away",worktreeLabel(w("discarded")))
    }
}
