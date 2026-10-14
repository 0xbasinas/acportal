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
}
