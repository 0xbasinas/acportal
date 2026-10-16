package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class OrchestrationTest {
    private fun t(id:String,parent:String?=null,at:Long=0,state:String="working")=TaskSummary(id=id,parent=parent,root=parent ?: id,agent="a$id",state=state,createdAt=at)

    @Test fun treesNestChildrenUnderParentsNewestRootFirstChildrenOldestFirst() {
        val rows=flattenTaskTree(buildTaskTrees(listOf(t("r1",at=1),t("c2","r1",3),t("c1","r1",2),t("g","c1",4),t("r2",at=5))))
        assertEquals(listOf("r2" to 0,"r1" to 0,"c1" to 1,"g" to 2,"c2" to 1),rows.map {it.task.id to it.indent})
    }
    @Test fun anOrphanBecomesARootAndACycleIsShownNotDropped() {
        val rows=flattenTaskTree(buildTaskTrees(listOf(t("o","gone"),t("x","y"),t("y","x"),t("s","s"))))
        assertEquals(setOf("o","x","y","s"),rows.map {it.task.id}.toSet())
        assertEquals(4,rows.size)
    }
    @Test fun hostTaskListDecodesIncludingDelegateAndUnknownFields() {
        val json="""[{"id":"11111111-1111-1111-1111-111111111111","parent":null,"root":"11111111-1111-1111-1111-111111111111","depth":0,"agent":"goose","scope":"/w","rights":{"read":true,"write":false,"terminal":false},"state":"input_required","session":null,"createdAt":1,"updatedAt":2,"summary":null,"stopReason":null,"traceId":"t","pendingDecisions":2,"delegate":"not_connected","future":1}]"""
        val task=WireJson.decodeFromString<List<TaskSummary>>(json).single()
        assertEquals(2,task.pendingDecisions);assertEquals("not_connected",task.delegate);assertNull(task.costUsd)
        assertEquals("Delegation tools not connected",delegateLabel(task.delegate))
        assertNull(delegateLabel(null))
    }
    @Test fun detailDecodesNeedsDecisionAndOutcomesReadPlainly() {
        val json="""{"id":"i","root":"i","agent":"w","state":"input_required","rights":{"read":true,"write":true,"terminal":false},"needsDecision":[
            {"requestId":7,"digest":"d","source":"host","kind":"edit","risk":"medium","facts":{},"decidable":true,"expiresInSeconds":20},
            {"requestId":"x","source":"agent","kind":"other","risk":"high","decidable":false,"reason":"risk_too_high"}]}"""
        val detail=WireJson.decodeFromString<TaskDetail>(json)
        assertEquals(listOf("Medium risk · the supervising agent may decide","High risk · too risky for the supervisor"),detail.needsDecision.map(::supervisorOutcomeLabel))
        assertEquals("read, write",taskRightsLabel(detail.rights))
        assertEquals("Unrated risk · only a person can answer",supervisorOutcomeLabel(TaskDecision(reason="human_only")))
    }
    @Test fun statesMapToLabelsAndOpenness() {
        assertEquals("Needs a decision",taskStateLabel("input_required"))
        assertTrue(taskIsOpen("working"));assertFalse(taskIsOpen("completed"));assertFalse(taskIsOpen("canceled"))
        assertEquals("odd",taskStateLabel("odd"))
    }
    @Test fun sessionsCarryTheirPeerAndOlderHostsOmitIt() {
        assertEquals("far",WireJson.decodeFromString<SessionInfo>("""{"id":"s","agentId":"a","acpSessionId":"x","workspace":"/w","remotePeer":"far"}""").remotePeer)
        assertNull(WireJson.decodeFromString<SessionInfo>("""{"id":"s","agentId":"a","acpSessionId":"x","workspace":"/w"}""").remotePeer)
    }
    @Test fun worktreesDecodeFromTheHostRecordAndReadPlainly() {
        val json="""{"id":"i","root":"r","agent":"w","state":"canceled","worktree":{"path":"/s/worktrees/x/src","repository":"/r","branch":"acpd/task-abc","base":"0123","sub":"src","baseDirty":true,"outcome":"snapshotted"}}"""
        val w=WireJson.decodeFromString<TaskDetail>(json).worktree!!
        assertEquals("acpd/task-abc · Removed; unsaved work committed to the branch · started without the parent's unsaved changes",worktreeLabel(w))
        assertEquals("acpd/task-abc · In use",worktreeLabel(w.copy(outcome="active",baseDirty=false)))
        assertNull(WireJson.decodeFromString<TaskDetail>("""{"id":"i","root":"r","agent":"w","state":"working"}""").worktree)
    }
    @Test fun onlyFinishedTasksWithAnUnmergedBranchOfferAMerge() {
        val w=TaskWorktree(branch="acpd/task-abc",outcome="snapshotted")
        assertTrue(canMergeWorktree("completed",w))
        assertTrue(canMergeWorktree("completed",w.copy(outcome="active")))
        assertTrue(canMergeWorktree("canceled",w.copy(outcome="branch_kept")))
        assertFalse(canMergeWorktree("working",w))
        assertFalse(canMergeWorktree("input_required",w))
        for(o in listOf("removed","merged","kept_dirty","releasing","cleanup_failed")) assertFalse(o,canMergeWorktree("completed",w.copy(outcome=o)))
        assertFalse(canMergeWorktree("completed",null))
        assertEquals("acpd/task-abc · Merged into the parent's folder (0123456)",worktreeLabel(w.copy(outcome="merged",merged="0123456789")))
        assertEquals("abc",WireJson.decodeFromString<TaskMergeResult>("""{"merged":"abc","branch":"b","into":"/r"}""").merged)
    }
}
