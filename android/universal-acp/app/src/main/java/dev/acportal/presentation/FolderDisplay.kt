package dev.acportal.presentation

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow

private val windowsDrive=Regex("^[A-Za-z]:([/\\\\].*)?$")

/** Folder name to show first: the host's name, else the last path segment, else the path. */
internal fun folderTitle(name:String,path:String):String {
    if(name.isNotBlank())return name
    val trimmed=path.trimEnd('/','\\')
    return trimmed.substringAfterLast('/').substringAfterLast('\\').ifBlank {path}
}

/**
 * Presentation form of a host path. A Windows path (drive letter) is shown with backslashes and an
 * upper-case drive; anything else is unchanged. Only for display: the original string is what the host receives.
 */
internal fun displayPath(raw:String):String {
    // Older hosts report the Windows verbatim form; show it without the prefix.
    val path=when {
        raw.startsWith("\\\\?\\UNC\\") -> "\\\\"+raw.removePrefix("\\\\?\\UNC\\")
        raw.startsWith("\\\\?\\") && raw.length>5 && raw[5]==':' -> raw.removePrefix("\\\\?\\")
        else -> raw
    }
    if(!windowsDrive.matches(path))return path
    return (path.substring(0,1).uppercase()+path.substring(1)).replace('/','\\')
}

/** Secondary path line: readable proportional text, one line, shortened in the middle. */
@Composable internal fun FolderPathText(path:String,modifier:Modifier=Modifier) {
    Text(displayPath(path),modifier,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.MiddleEllipsis)
}
