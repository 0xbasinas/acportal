package dev.acportal.presentation

import dev.acportal.protocol.*
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.IOException

class TaskMergeTest {
    private val merged=TaskMergeResult(merged="0123456789abcdef",branch="acpd/task-1")
    private val summary=TaskSummary(id="i",root="i",agent="w",state="completed")
    private val detail=TaskDetail(id="i",root="i",agent="w",state="completed")
    private val before=TasksUiState(hostId="h",tasks=listOf(summary.copy(state="working")),detail=null,loading=true)

    @Test fun aSuccessfulMergeSaysSoAndShowsTheFreshTaskState() {
        val state=MutableStateFlow(before)
        runBlocking {mergeAndRefresh(state,{merged},{listOf(summary)},{detail})}
        assertEquals("Merged acpd/task-1 into the parent's folder (0123456).",state.value.notice)
        assertNull(state.value.error)
        assertEquals("completed",state.value.tasks?.single()?.state)
        assertEquals(detail,state.value.detail)
    }
    @Test fun aRefreshThatFailsAfterTheMergeKeepsTheNoticeAndSaysTheMergeIsDone() {
        for(failure in listOf<suspend ()->Nothing>({throw IOException("down")},{throw IllegalStateException("bad json")})) {
            val state=MutableStateFlow(before)
            runBlocking {mergeAndRefresh(state,{merged},failure,{detail})}
            assertEquals("Merged acpd/task-1 into the parent's folder (0123456).",state.value.notice)
            assertTrue(state.value.error!!.startsWith("The merge is done, but"))
            assertEquals("working",state.value.tasks?.single()?.state)
        }
    }
    @Test fun aMergeTheHostRefusesChangesNothingAndTheErrorReachesTheCaller() {
        val state=MutableStateFlow(before)
        val error=assertThrows(IllegalStateException::class.java) {runBlocking {mergeAndRefresh(state,{error("The parent's folder has uncommitted changes.")},{listOf(summary)},{detail})}}
        assertTrue(error.message!!.contains("uncommitted"))
        assertEquals(before,state.value)
    }
    @Test fun cancellationDuringTheRefreshIsNotTurnedIntoAnError() {
        val state=MutableStateFlow(before)
        assertThrows(CancellationException::class.java) {runBlocking {mergeAndRefresh(state,{merged},{throw CancellationException("left the screen")},{detail})}}
        assertNull(state.value.error)
        assertNotNull(state.value.notice)
    }
}
