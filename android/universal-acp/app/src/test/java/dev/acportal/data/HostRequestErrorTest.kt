package dev.acportal.data

import org.junit.Assert.*
import org.junit.Test

class HostRequestErrorTest {
    @Test fun rejectedCreationOffersExplicitRecoveryWithoutClaimingTheCause() {
        val message=hostRequestError(400,"invalid_request","POST","/v1/sessions")
        assertTrue(message.contains("existing allowed folder"))
        assertTrue(message.contains("agent and MCP settings"))
        assertTrue(message.contains("press Start session again"))
        assertEquals("Host request failed (400).",hostRequestError(400,"invalid_request","GET","/v1/workspaces/browse"))
        assertEquals("Host request failed (400).",hostRequestError(400,"invalid_request","POST","/v1/pair"))
    }
    @Test fun specificTransportAndCredentialFailuresTakePrecedence() {
        assertTrue(hostRequestError(400,"unsupported_mcp_transport","POST","/v1/sessions").contains("Disable that server"))
        assertTrue(hostRequestError(401,"unauthorized","POST","/v1/sessions").contains("Pair this host again"))
        assertTrue(hostRequestError(409,"controller_conflict","POST","/v1/sessions").contains("controller"))
    }
    @Test fun aMergeConflictNamesTheFilesAndAManyFileConflictStaysShort() {
        val few=hostRequestError(409,"merge_conflict","POST","/v1/tasks/1/merge",listOf("a.rs","b.rs"))
        assertTrue(few.contains("a.rs, b.rs"))
        assertTrue(few.contains("merge the branch by hand"))
        val many=hostRequestError(409,"merge_conflict","POST","/v1/tasks/1/merge",(1..9).map {"f$it.rs"})
        assertTrue(many.contains("f1.rs, f2.rs, f3.rs, f4.rs, f5.rs and 4 more"))
        assertFalse(many.contains("f6.rs"))
        // No names (an older host, or git gave none): the plain message, still with its advice.
        assertTrue(hostRequestError(409,"merge_conflict","POST","/v1/tasks/1/merge").contains("merge the branch by hand"))
        // A conflict code only means this on the merge route.
        assertEquals("This session already has a controller.",hostRequestError(409,"merge_conflict","POST","/v1/sessions",listOf("a.rs")))
    }
    @Test fun aConflictGivesTheCommandToMergeByHandAndNeverOneThatCouldBeMisread() {
        val path="/v1/tasks/1/merge"
        val message=hostRequestError(409,"merge_conflict","POST",path,listOf("a.rs"),"acpd/task-0123456789ab","C:\\work\\repo")
        assertTrue(message,message.endsWith("Command: git -C \"C:\\work\\repo\" merge acpd/task-0123456789ab"))
        // Only the branch known: still says it conflicts, with no half-built command.
        val branchOnly=hostRequestError(409,"merge_conflict","POST",path,emptyList(),"acpd/task-1","")
        assertTrue(branchOnly.contains("merge the branch by hand"))
        assertFalse(branchOnly.contains("Command:"))
        // A name with a space or a quote is never put into a command line.
        assertFalse(hostRequestError(409,"merge_conflict","POST",path,listOf("a.rs"),"bad branch","/r").contains("Command:"))
        assertFalse(hostRequestError(409,"merge_conflict","POST",path,listOf("a.rs"),"b","/r\"; rm -rf /").contains("Command:"))
    }
    @Test fun discardRefusalsSayWhatHappened() {
        val path="/v1/tasks/1/discard"
        assertEquals("This task's work was already thrown away.",hostRequestError(409,"already_discarded","POST",path))
        assertTrue(hostRequestError(409,"nothing_to_discard","POST",path).contains("nothing left to discard"))
        assertTrue(hostRequestError(500,"git_failed","POST",path).contains("Try again"))
        assertTrue(hostRequestError(409,"already_discarded","POST","/v1/tasks/1/merge").contains("nothing to merge"))
    }
    @Test fun rejectedPairingOffersANewCodeWithoutClaimingCredentialExpiry() {
        val message=hostRequestError(401,"pairing_failed","POST","/v1/pair")
        assertTrue(message.contains("Pairing code was rejected"))
        assertTrue(message.contains("acpd pair"))
        assertFalse(message.contains("Credentials expired"))
    }
    @Test fun gitRefusalsAreExplained() {
        assertTrue(hostRequestError(403,"path_not_allowed","POST","/v1/git/worktree-add").contains("does not allow that location"))
        assertTrue(hostRequestError(404,"unknown_git_operation","POST","/v1/git/diff-new").contains("older than the app"))
        assertEquals("Host request failed (403).",hostRequestError(403,"path_not_allowed","POST","/v1/sessions"))
    }
}
