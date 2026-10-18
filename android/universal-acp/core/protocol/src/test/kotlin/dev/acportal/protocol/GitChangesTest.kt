package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class GitChangesTest {
    @Test fun statusSplitsStagedUnstagedAndNewFiles() {
        val out = "## main...origin/main [ahead 1]\n M src/calc.py\nM  docs/protocol.md\nMM src/App.tsx\n?? src/gitRepos.ts\nA  added.txt\n"
        val status = parseGitStatus(out)
        assertEquals("main", status.branch)
        assertEquals("origin/main", status.upstream)
        assertEquals(listOf("src/calc.py", "src/App.tsx", "src/gitRepos.ts"), status.unstaged.map { it.path })
        assertEquals(listOf("docs/protocol.md", "src/App.tsx", "added.txt"), status.staged.map { it.path })
        assertEquals("new", status.files.single { it.path == "src/gitRepos.ts" }.label)
        assertTrue(status.files.single { it.path == "src/gitRepos.ts" }.untracked)
        assertEquals("modified", status.files.single { it.path == "src/calc.py" }.label)
        assertEquals("added", status.files.single { it.path == "added.txt" }.label)
    }
    @Test fun statusHandlesDetachedHeadsNewRepositoriesRenamesAndConflicts() {
        assertNull(parseGitStatus("## HEAD (no branch)\n").branch)
        assertEquals("main", parseGitStatus("## No commits yet on main\n").branch)
        val renamed = parseGitStatus("## main\nR  old.txt -> new.txt\nUU both.txt\n")
        assertEquals("new.txt", renamed.files[0].path)
        assertEquals("renamed", renamed.files[0].label)
        assertTrue(renamed.files[1].conflicted)
        assertFalse(renamed.files[1].staged || renamed.files[1].unstaged)
        assertEquals("conflict", renamed.files[1].label)
    }
    @Test fun diffIsSplitIntoFilesWithCountsAndLines() {
        val out = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n+more\ndiff --git a/b.txt b/b.txt\nnew file mode 100644\n--- /dev/null\n+++ b/b.txt\n@@ -0,0 +1 @@\n+hello\n"
        val files = parseUnifiedDiff(out)
        assertEquals(listOf("a.txt", "b.txt"), files.map { it.path })
        assertEquals(2, files[0].added); assertEquals(1, files[0].removed)
        assertEquals(listOf('@', ' ', '-', '+', '+'), files[0].lines.map { it.kind })
        assertEquals("keep", files[0].lines[1].text)
        assertEquals(1, files[1].added)
        assertEquals("hello", files[1].lines.last().text)
    }
    @Test fun aLineStartingWithPlusPlusPlusInsideAHunkIsContentNotAHeader() {
        val out = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+++ b\n"
        val file = parseUnifiedDiff(out).single()
        assertEquals("x", file.path)
        assertEquals(1, file.added)
        assertEquals("++ b", file.lines.last().text)
    }
    @Test fun binaryAndEmptyOutputGiveNoLines() {
        assertTrue(parseUnifiedDiff("").isEmpty())
        val binary = parseUnifiedDiff("diff --git a/i.png b/i.png\nBinary files a/i.png and b/i.png differ\n").single()
        assertTrue(binary.lines.isEmpty())
    }
    @Test fun theBaseBranchIsRecognisedWithOrWithoutTheRemotePrefix() {
        assertTrue(isBaseBranch("main", "main"))
        assertTrue(isBaseBranch("origin/main", "main"))
        assertFalse(isBaseBranch("main", "feature/git"))
        assertFalse(isBaseBranch("main", null))
    }
}

class HunkPatchTest {
    private val diff = "diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n@@ -10,2 +10,3 @@\n ten\n+eleven\n twelve\n\\ No newline at end of file\n"
    @Test fun aHunkBecomesAPatchWithTheFileHeaderAndItsOwnLinesOnly() {
        val file = parseUnifiedDiff(diff).single()
        assertEquals(2, file.hunks.size)
        val second = hunkPatch(file, 1)!!
        assertEquals("diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n@@ -10,2 +10,3 @@\n ten\n+eleven\n twelve\n\\ No newline at end of file\n", second)
        assertFalse(second.contains("-old"))
        assertTrue(hunkPatch(file, 0)!!.contains("-old"))
    }
    @Test fun noPatchForAMissingHunkOrABinaryFile() {
        val file = parseUnifiedDiff(diff).single()
        assertNull(hunkPatch(file, 2))
        val binary = parseUnifiedDiff("diff --git a/i.png b/i.png\nindex 1..2 100644\nBinary files a/i.png and b/i.png differ\n").single()
        assertNull(hunkPatch(binary, 0))
        assertTrue(binary.hunks.isEmpty())
    }
}
