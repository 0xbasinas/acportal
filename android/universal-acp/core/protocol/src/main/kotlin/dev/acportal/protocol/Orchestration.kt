package dev.acportal.protocol

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

/** Host feature: delegation is enabled, so `/v1/tasks` has something to show. Off by default. */
const val HOST_FEATURE_ORCHESTRATOR = "orchestrator"

@Serializable data class TaskRights(val read:Boolean=false,val write:Boolean=false,val terminal:Boolean=false)

/** One `/v1/tasks` entry. Never carries prompts or results: only what the task was and how it ended. */
@Serializable
data class TaskSummary(
    val id:String, val parent:String?=null, val root:String, val depth:Int=0, val agent:String,
    val scope:String="", val rights:TaskRights=TaskRights(), val state:String,
    val session:String?=null, val createdAt:Long=0, val updatedAt:Long=0,
    val summary:String?=null, val stopReason:String?=null, val traceId:String?=null,
    val costUsd:Double?=null, val pendingDecisions:Int=0,
    /** `connected`, `not_connected`, or absent when the task was not offered the delegation tools. */
    val delegate:String?=null,
)

/** A request in `/v1/tasks/{id}` `needsDecision`. Facts are host words; the phone may always answer it in the session. */
@Serializable
data class TaskDecision(
    val requestId:JsonElement?=null, val source:String?=null, val kind:String?=null, val risk:String?=null,
    val facts:JsonElement?=null, val decidable:Boolean=false, val expiresInSeconds:Long?=null,
    val reason:String?=null, val note:String?=null,
)
/** The task's own git worktree (phase 4), when it was delegated with worktree isolation. */
@Serializable data class TaskWorktree(val branch:String,val path:String="",val baseDirty:Boolean=false,val outcome:String="active",val merged:String?=null)
/** What the host answered to a merge: the merge commit and the branch it came from. */
@Serializable data class TaskMergeResult(val merged:String,val branch:String)
/** What the host answered to a discard: the branch whose work was thrown away. */
@Serializable data class TaskDiscardResult(val discarded:String)
@Serializable
data class TaskDetail(
    val id:String, val parent:String?=null, val root:String, val depth:Int=0, val agent:String,
    val scope:String="", val rights:TaskRights=TaskRights(), val state:String,
    val session:String?=null, val summary:String?=null, val stopReason:String?=null,
    val costUsd:Double?=null, val delegate:String?=null, val needsDecision:List<TaskDecision> = emptyList(),
    val worktree:TaskWorktree?=null,
)

data class TaskNode(val task:TaskSummary,val children:List<TaskNode>)
data class TaskRow(val task:TaskSummary,val indent:Int)

/**
 * Builds trees from the flat list. A task whose parent is missing (pruned records) becomes a root, and
 * a cycle in bad data cannot recurse forever: each id is placed once.
 */
fun buildTaskTrees(tasks:List<TaskSummary>):List<TaskNode> {
    val ids=tasks.map {it.id}.toSet()
    val byParent=tasks.groupBy {t->t.parent?.takeIf {it in ids && it!=t.id}}
    val placed=mutableSetOf<String>()
    fun node(t:TaskSummary):TaskNode {placed+=t.id;return TaskNode(t,byParent[t.id].orEmpty().filter {it.id !in placed}.sortedBy {it.createdAt}.map(::node))}
    val roots=byParent[null].orEmpty().sortedByDescending {it.createdAt}.map(::node).toMutableList()
    // Anything left is in a cycle; show it flat rather than drop it.
    for(t in tasks) if(t.id !in placed)roots+=node(t)
    return roots
}
fun flattenTaskTree(nodes:List<TaskNode>,indent:Int=0):List<TaskRow> = nodes.flatMap {listOf(TaskRow(it.task,indent))+flattenTaskTree(it.children,indent+1)}

fun taskStateLabel(state:String)=when(state) {
    "submitted"->"Queued";"working"->"Working";"input_required"->"Needs a decision";"auth_required"->"Needs sign-in"
    "completed"->"Completed";"failed"->"Failed";"canceled"->"Cancelled";"rejected"->"Refused";else->state}
fun taskIsOpen(state:String)=state in setOf("submitted","working","input_required","auth_required")
fun delegateLabel(delegate:String?)=when(delegate) {"connected"->"Delegation tools connected";"not_connected"->"Delegation tools not connected";else->null}

/** What the supervisor may or may not do with a waiting request, in plain words. */
fun supervisorOutcomeLabel(decision:TaskDecision):String {
    val risk=decision.risk?.replaceFirstChar {it.uppercase()} ?: "Unrated"
    if(decision.decidable)return "$risk risk · the supervising agent may decide"
    val why=when(decision.reason) {
        "human_only"->"only a person can answer"; "escalated"->"escalated to you"
        "supervisor_disabled"->"supervisor approvals are off"; "paused"->"supervision is paused"
        "risk_too_high"->"too risky for the supervisor"; "expired"->"supervisor window ended"
        null->"waiting for you"; else->decision.reason}
    return "$risk risk · $why"
}
/** What happened to a task's worktree, in plain words. */
fun worktreeLabel(w:TaskWorktree):String {
    val what=when(w.outcome) {
        "active"->"In use";"releasing"->"Being cleaned up";"removed"->"Removed, no changes"
        "branch_kept"->"Removed; commits kept on the branch";"snapshotted"->"Removed; unsaved work committed to the branch"
        "kept_dirty"->"Kept on disk with unsaved work";"cleanup_failed"->"Cleanup failed; check git worktree list"
        "merged"->"Merged into the parent's folder"+(w.merged?.let {" (${it.take(7)})"} ?: "")
        "discarded"->"Discarded; its work was thrown away";else->w.outcome}
    return "${w.branch} · $what"+if(w.baseDirty)" · started without the parent's unsaved changes" else ""
}
/** A finished task whose worktree branch may hold work the parent's folder lacks can be merged. */
fun canMergeWorktree(state:String,w:TaskWorktree?):Boolean = w!=null && !taskIsOpen(state) && w.outcome in setOf("active","branch_kept","snapshotted")
/** A finished task's kept work (or live worktree) can be thrown away; a merged, discarded or empty one cannot. */
fun canDiscardWorktree(state:String,w:TaskWorktree?):Boolean = w!=null && !taskIsOpen(state) && w.outcome in setOf("active","branch_kept","snapshotted","kept_dirty","cleanup_failed")
fun taskRightsLabel(r:TaskRights)=listOfNotNull("read".takeIf {r.read},"write".takeIf {r.write},"terminal".takeIf {r.terminal}).joinToString(", ").ifEmpty {"no rights"}
