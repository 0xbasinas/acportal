package dev.acportal.presentation

import dev.acportal.data.taskPath
import dev.acportal.data.hostRequestError
import dev.acportal.protocol.*
import org.junit.Assert.*
import org.junit.Test

class TaskScreensTest {
    @Test fun rowLineShowsStateWaitingDelegateAndHostReason() {
        val task=TaskSummary(id="i",root="i",agent="w",state="failed",pendingDecisions=1,delegate="connected",summary="deadline")
        assertEquals("Failed · 1 request waiting · Delegation tools connected · deadline",taskRowSupporting(task))
        assertEquals("Working",taskRowSupporting(task.copy(state="working",pendingDecisions=0,delegate=null,summary=null)))
        assertTrue(taskRowSupporting(task.copy(pendingDecisions=3)).contains("3 requests waiting"))
    }
    @Test fun peerSessionsNameTheirPeer() {
        val s=SessionInfo(id="s",agentId="goose",acpSessionId="x",workspace="/w",remotePeer="lab")
        assertTrue(sessionRowSupporting(s).startsWith("goose · on peer lab · "))
        assertFalse(sessionRowSupporting(s.copy(remotePeer=null)).contains("peer"))
    }
    @Test fun taskIdsMustBeUuidsAndTaskErrorsAreSpecific() {
        assertEquals("11111111-1111-1111-1111-111111111111",taskPath("11111111-1111-1111-1111-111111111111"))
        assertThrows(IllegalArgumentException::class.java) {taskPath("../sessions")}
        assertEquals("This task has already finished.",hostRequestError(409,"task_not_open","POST","/v1/tasks/x/cancel"))
        assertEquals("This session already has a controller.",hostRequestError(409,"","GET","/v1/sessions/x/connect"))
    }
    @Test fun mergeRefusalsSayWhatToDo() {
        val path="/v1/tasks/11111111-1111-1111-1111-111111111111/merge"
        assertTrue(hostRequestError(409,"parent_dirty","POST",path).startsWith("The parent's folder has uncommitted changes"))
        assertTrue(hostRequestError(409,"merge_conflict","POST",path).contains("Nothing was changed"))
        assertEquals("This task's branch was already merged.",hostRequestError(409,"already_merged","POST",path))
        assertEquals("The task's branch has nothing the parent's folder lacks.",hostRequestError(409,"nothing_to_merge","POST",path))
    }
}
