package dev.acportal.protocol

/** One line of \`git status --porcelain=v1 -b -uall\`: the index column, the work-tree column and the path. */
data class GitFile(val path: String, val index: Char, val worktree: Char) {
    val untracked: Boolean get() = index == '?'
    val conflicted: Boolean get() = index == 'U' || worktree == 'U' || (index == 'A' && worktree == 'A') || (index == 'D' && worktree == 'D')
    /** Has a change recorded in the index (shown under Staged). */
    val staged: Boolean get() = !untracked && index != ' ' && !conflicted
    /** Has a change in the work tree that is not staged yet (shown under Unstaged), new files included. */
    val unstaged: Boolean get() = untracked || (worktree != ' ' && !conflicted)
    val label: String get() = when {
        untracked -> "new"
        conflicted -> "conflict"
        else -> when (if (staged && !unstaged) index else worktree) { 'A' -> "added"; 'D' -> "deleted"; 'R' -> "renamed"; else -> "modified" }
    }
}

data class GitStatus(val branch: String?, val upstream: String?, val files: List<GitFile>) {
    val unstaged: List<GitFile> get() = files.filter { it.unstaged }
    val staged: List<GitFile> get() = files.filter { it.staged }
}

/** \`## main...origin/main [ahead 1]\` then \`XY path\` lines. A detached head has no branch. */
fun parseGitStatus(out: String): GitStatus {
    var branch: String? = null
    var upstream: String? = null
    val files = ArrayList<GitFile>()
    for (line in out.lines()) {
        if (line.startsWith("## ")) {
            val head = line.removePrefix("## ").substringBefore(" [")
            val name = head.substringBefore("...")
            upstream = head.substringAfter("...", "").ifEmpty { null }
            branch = when {
                name.startsWith("No commits yet on ") -> name.removePrefix("No commits yet on ")
                name.startsWith("HEAD (no branch)") -> null
                else -> name
            }
        } else if (line.length > 3 && line[2] == ' ') {
            val path = line.substring(3).substringAfter(" -> ").trim().removeSurrounding("\"")
            files += GitFile(path, line[0], line[1])
        }
    }
    return GitStatus(branch, upstream, files)
}

/** One line of a unified diff: \`+\` added, \`-\` removed, space for context; hunk headers are kept as \`@\`. */
data class GitDiffLine(val kind: Char, val text: String)
/**
 * One file of a diff. [header] holds the lines before the first hunk (diff --git, index, ---, +++) and
 * [hunks] each hunk as git wrote it, so one hunk can be sent back to git as a patch.
 */
data class FileDiffText(val path: String, val added: Int, val removed: Int, val lines: List<GitDiffLine>, val header: List<String> = emptyList(), val hunks: List<List<String>> = emptyList())

/** A patch of one hunk of [file], for git apply --cached (stage) or --cached -R (unstage). Null when the file cannot be patched by hunk. */
fun hunkPatch(file: FileDiffText, hunk: Int): String? {
    val body = file.hunks.getOrNull(hunk) ?: return null
    if (file.header.none { it.startsWith("+++ ") } || file.header.any { it.startsWith("Binary files") || it.startsWith("GIT binary patch") }) return null
    return (file.header + body).joinToString("\n") + "\n"
}

/** Parses \`git diff\` output into one entry per file. Binary files have no lines. */
fun parseUnifiedDiff(out: String): List<FileDiffText> {
    val files = ArrayList<FileDiffText>()
    var path: String? = null
    var lines = ArrayList<GitDiffLine>()
    var added = 0
    var removed = 0
    var inHunk = false
    var header = ArrayList<String>()
    var hunks = ArrayList<ArrayList<String>>()
    fun flush() {
        path?.let { files += FileDiffText(it, added, removed, lines, header, hunks) }
        path = null; lines = ArrayList(); added = 0; removed = 0; inHunk = false; header = ArrayList(); hunks = ArrayList()
    }
    for (line in out.lines()) {
        if (line.startsWith("diff --git ")) {
            flush()
            path = line.removePrefix("diff --git ").substringAfterLast(" b/").trim()
            header += line
        } else if (!inHunk && line.startsWith("+++ ")) {
            val target = line.removePrefix("+++ ").trim()
            if (target != "/dev/null") path = target.removePrefix("b/")
            header += line
        } else if (line.startsWith("@@")) {
            inHunk = true
            lines += GitDiffLine('@', line)
            hunks += arrayListOf(line)
        } else if (!inHunk && path != null && line.isNotEmpty()) {
            header += line
        } else if (inHunk && path != null && line.isNotEmpty()) {
            hunks.lastOrNull()?.add(line)
            when (line[0]) {
                '+' -> { added++; lines += GitDiffLine('+', line.substring(1)) }
                '-' -> { removed++; lines += GitDiffLine('-', line.substring(1)) }
                ' ' -> lines += GitDiffLine(' ', line.substring(1))
                else -> {}
            }
        }
    }
    flush()
    return files
}

/** The default pull request base is \`origin/main\` or \`main\`; on that branch there is nothing to open. */
fun isBaseBranch(base: String, current: String?): Boolean =
    current != null && base.trim().removePrefix("origin/") == current
