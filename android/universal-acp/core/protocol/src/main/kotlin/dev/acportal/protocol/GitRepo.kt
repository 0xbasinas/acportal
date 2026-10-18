package dev.acportal.protocol

private const val SEP = '\u001f'

/** One branch from `git branch --all --format=...`. [remote] is a branch of `origin` or `upstream`. */
data class GitBranch(val name: String, val head: String, val current: Boolean, val track: String, val updated: String) {
    val remote: Boolean get() = name.startsWith("origin/") || name.startsWith("upstream/")
}

fun parseBranches(out: String): List<GitBranch> = out.lines().mapNotNull { line ->
    val parts = line.split(SEP)
    if (parts.size < 5 || parts[0].isBlank()) return@mapNotNull null
    // The symbolic head of a remote ("origin" or "origin/HEAD") is not a branch to switch to.
    if (parts[0].endsWith("/HEAD") || parts[0] == "origin" || parts[0] == "upstream" || parts[0].startsWith("(HEAD")) return@mapNotNull null
    GitBranch(parts[0], parts[1], parts[2].trim() == "*", parts[3].trim().removeSurrounding("[", "]"), parts[4])
}

/** One entry of `git worktree list --porcelain`. */
data class GitWorktree(val path: String, val head: String, val branch: String?, val bare: Boolean, val detached: Boolean)

fun parseWorktrees(out: String): List<GitWorktree> {
    val result = ArrayList<GitWorktree>()
    var path: String? = null; var head = ""; var branch: String? = null; var bare = false; var detached = false
    fun flush() { path?.let { result += GitWorktree(it, head, branch, bare, detached) }; path = null; head = ""; branch = null; bare = false; detached = false }
    for (line in out.lines()) {
        when {
            line.startsWith("worktree ") -> { flush(); path = line.removePrefix("worktree ").trim() }
            line.startsWith("HEAD ") -> head = line.removePrefix("HEAD ").trim().take(8)
            line.startsWith("branch ") -> branch = line.removePrefix("branch ").trim().removePrefix("refs/heads/")
            line == "bare" -> bare = true
            line == "detached" -> detached = true
        }
    }
    flush()
    return result
}

/** A commit from `git log --pretty=%H %P %d %s %an %cr` (the host's `log-dag`). */
data class GraphCommit(val hash: String, val parents: List<String>, val refs: String, val subject: String, val author: String, val time: String)

fun parseLogDag(out: String): List<GraphCommit> = out.lines().mapNotNull { line ->
    val parts = line.split(SEP)
    if (parts.size < 6 || parts[0].isBlank()) return@mapNotNull null
    GraphCommit(parts[0], parts[1].split(' ').filter { it.isNotBlank() }, parts[2].trim().removeSurrounding("(", ")"), parts[3], parts[4], parts[5])
}

/**
 * One row of the drawn history. [node] is the commit's lane; [through] are the lanes a line passes
 * straight through the row; [joining] are the lanes of the row above that end at this commit;
 * [parentLanes] are the lanes below that its parents continue in.
 */
data class GraphRow(val commit: GraphCommit, val node: Int, val through: List<Int>, val joining: List<Int>, val parentLanes: List<Int>)

/** Lays out [commits] (newest first, children before their parents) in lanes. */
fun layoutGraph(commits: List<GraphCommit>): List<GraphRow> {
    val lanes = ArrayList<String?>()
    val rows = ArrayList<GraphRow>(commits.size)
    fun free(): Int { val i = lanes.indexOf(null); return if (i >= 0) i else { lanes.add(null); lanes.size - 1 } }
    for (commit in commits) {
        val joining = lanes.indices.filter { lanes[it] == commit.hash }
        val node = joining.firstOrNull() ?: free().also { lanes[it] = commit.hash }
        val through = lanes.indices.filter { lanes[it] != null && it != node && it !in joining }
        for (i in joining) lanes[i] = null
        val parentLanes = ArrayList<Int>()
        commit.parents.forEachIndexed { index, parent ->
            val existing = lanes.indexOf(parent)
            val lane = when {
                existing >= 0 -> existing
                index == 0 -> { lanes[node] = parent; node }
                else -> free().also { lanes[it] = parent }
            }
            parentLanes += lane
        }
        while (lanes.isNotEmpty() && lanes.last() == null) lanes.removeAt(lanes.lastIndex)
        rows += GraphRow(commit, node, through, joining, parentLanes)
    }
    return rows
}

/** One entry of `git stash list` (the host's `stash-list`): the reference, its message and when it was made. */
data class GitStash(val ref: String, val message: String, val time: String)

fun parseStashes(out: String): List<GitStash> = out.lines().mapNotNull { line ->
    val parts = line.split(SEP)
    if (parts.size < 3 || !parts[0].startsWith("stash@{")) return@mapNotNull null
    GitStash(parts[0], parts[1], parts[2])
}

/** The host's `merging` reply is "yes" while a merge is waiting to be finished or aborted. */
fun isMerging(out: String?): Boolean = out?.trim() == "yes"
