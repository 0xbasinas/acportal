package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class GitRepoTest {
    private val sep = "\u001f"
    @Test fun branchesSkipTheRemoteHeadAndMarkTheCurrentOne() {
        val out = "main${sep}abc1234$sep*$sep[ahead 1]${sep}2 hours ago\nfeature/git${sep}def5678$sep$sep${sep}1 day ago\norigin${sep}abc1234$sep$sep${sep}2 hours ago\norigin/main${sep}abc1234$sep$sep${sep}2 hours ago\n"
        val branches = parseBranches(out)
        assertEquals(listOf("main", "feature/git", "origin/main"), branches.map { it.name })
        assertTrue(branches[0].current)
        assertEquals("ahead 1", branches[0].track)
        assertFalse(branches[1].remote)
        assertTrue(branches[2].remote)
    }
    @Test fun worktreesListTheirBranchAndDetachedHeads() {
        val out = "worktree /work/app\nHEAD 1234567890abcdef\nbranch refs/heads/main\n\nworktree /work/app-feature\nHEAD fedcba0987654321\nbranch refs/heads/feature\n\nworktree /work/app-detached\nHEAD 0000000000000000\ndetached\n"
        val trees = parseWorktrees(out)
        assertEquals(listOf("/work/app", "/work/app-feature", "/work/app-detached"), trees.map { it.path })
        assertEquals("main", trees[0].branch)
        assertEquals("12345678", trees[0].head)
        assertTrue(trees[2].detached)
        assertNull(trees[2].branch)
    }
    private fun commit(hash: String, vararg parents: String) = GraphCommit(hash, parents.toList(), "", "subject $hash", "me", "now")
    @Test fun aStraightHistoryUsesOneLane() {
        val rows = layoutGraph(listOf(commit("c", "b"), commit("b", "a"), commit("a")))
        assertTrue(rows.all { it.node == 0 })
        assertEquals(listOf(0), rows[0].parentLanes)
        assertTrue(rows[2].parentLanes.isEmpty())
        assertEquals(listOf(0), rows[1].joining)
    }
    @Test fun aMergeOpensASecondLaneThatJoinsBackAtTheirCommonParent() {
        // m merges b (side) into a (main); both descend from base.
        val rows = layoutGraph(listOf(commit("m", "a", "b"), commit("a", "base"), commit("b", "base"), commit("base")))
        assertEquals(0, rows[0].node)
        assertEquals(listOf(0, 1), rows[0].parentLanes)
        // a stays on lane 0 while lane 1 runs through; b is on lane 1.
        assertEquals(0, rows[1].node); assertEquals(listOf(1), rows[1].through)
        assertEquals(1, rows[2].node); assertEquals(listOf(0), rows[2].through)
        // b's line bends into lane 0 below it, where base is reached.
        assertEquals(listOf(0), rows[2].parentLanes)
        assertEquals(0, rows[3].node)
        assertEquals(listOf(0), rows[3].joining)
    }
    @Test fun logLinesBecomeCommitsWithTheirParents() {
        val out = "h2${sep}h1 h0$sep (HEAD -> main)${sep}Merge things${sep}Me${sep}2 hours ago\nh1$sep$sep$sep" + "first${sep}Me${sep}1 day ago\n"
        val commits = parseLogDag(out)
        assertEquals(listOf("h1", "h0"), commits[0].parents)
        assertEquals("HEAD -> main", commits[0].refs)
        assertTrue(commits[1].parents.isEmpty())
    }
}

class GitStashAndConflictTest {
    private val sep = "\u001f"
    @Test fun stashesKeepTheirReferenceMessageAndAge() {
        val out = "stash@{0}${sep}WIP on main: abc1234 first${sep}2 minutes ago\nstash@{1}${sep}On feature: saved${sep}3 days ago\nnot a stash\n"
        val stashes = parseStashes(out)
        assertEquals(listOf("stash@{0}", "stash@{1}"), stashes.map { it.ref })
        assertEquals("On feature: saved", stashes[1].message)
        assertEquals("3 days ago", stashes[1].time)
        assertTrue(parseStashes("").isEmpty())
    }
    @Test fun conflictedFilesAreListedApartFromStagedAndUnstaged() {
        val status = parseGitStatus("## main\nUU both.txt\nAA added.txt\nDD gone.txt\n M plain.txt\n")
        assertEquals(listOf("both.txt", "added.txt", "gone.txt"), status.conflicts.map { it.path })
        assertEquals(listOf("plain.txt"), status.unstaged.map { it.path })
        assertTrue(status.staged.isEmpty())
    }
    @Test fun aMergeIsInProgressOnlyWhenTheHostSaysYes() {
        assertTrue(isMerging("yes\n"))
        assertFalse(isMerging(""))
        assertFalse(isMerging(null))
    }
}
