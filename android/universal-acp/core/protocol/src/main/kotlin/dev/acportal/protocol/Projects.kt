package dev.acportal.protocol

import kotlinx.serialization.Serializable

private const val VERBATIM = "\\\\?\\"
private const val VERBATIM_UNC = "\\\\?\\UNC\\"

/**
 * A Windows host lists project folders as `C:\x` but reports a session's folder in its canonical
 * form, `\\?\C:\x` (`\\?\UNC\srv\share` for a share). Both name the same folder, so the prefix is
 * dropped before paths are compared.
 */
fun plainPath(path: String): String = when {
    path.startsWith(VERBATIM_UNC) -> "\\\\" + path.removePrefix(VERBATIM_UNC)
    path.startsWith(VERBATIM) -> path.removePrefix(VERBATIM)
    else -> path
}
private fun normalPath(path: String): String = plainPath(path).replace('\\', '/').trimEnd('/').lowercase()

/** Whether [path] is [folder] or inside it (case-insensitive, either slash, as on Windows). */
fun pathInside(path: String, folder: String): Boolean {
    val a = normalPath(path)
    val b = normalPath(folder)
    return a == b || a.startsWith("$b/")
}

/**
 * Whether a session folder is one the host made for a session outside every project
 * (`<state>/scratch/<seconds>-<six hex>`). The host reports that root as `scratchRoot`; an older
 * host does not, so the shape of the name is used then. Such a folder can sit inside a project (a
 * development host keeps its state in the repository) and must never count as that project.
 */
fun isScratchFolder(workspace: String, scratchRoot: String?): Boolean {
    if (!scratchRoot.isNullOrBlank()) return pathInside(workspace, scratchRoot) && normalPath(workspace) != normalPath(scratchRoot)
    return Regex("(^|/)scratch/\\d+-[0-9a-f]{6}(/|$)").containsMatchIn(normalPath(workspace))
}

/** What this phone knows of a host's projects: the folders the host allows, and where it makes scratch folders. */
@Serializable
data class HostProjects(val roots: List<String> = emptyList(), val scratchRoot: String? = null)

/** The project folder a session runs in: the deepest allowed folder containing it, or null for no project. */
fun owningProject(workspace: String, projects: HostProjects): String? {
    if (isScratchFolder(workspace, projects.scratchRoot)) return null
    return projects.roots.filter { pathInside(workspace, it) }.maxByOrNull { normalPath(it).length }
}

fun folderName(path: String): String = plainPath(path).trimEnd('/', '\\').substringAfterLast('/').substringAfterLast('\\').ifEmpty { path }
