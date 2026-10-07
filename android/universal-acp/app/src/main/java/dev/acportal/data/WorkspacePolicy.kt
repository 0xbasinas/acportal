package dev.acportal.data

import java.util.Locale

fun workspacePolicyKey(hostId:String,path:String):String {
    var normalized=path.replace('\\','/')
    if(normalized.startsWith("//?/UNC/",ignoreCase=true))normalized="//"+normalized.substring(8)
    else if(normalized.startsWith("//?/"))normalized=normalized.substring(4)
    val windows=normalized.startsWith("//") || Regex("^[A-Za-z]:/.*").matches(normalized)
    normalized=normalized.trimEnd('/').ifEmpty {"/"}
    if(windows)normalized=normalized.lowercase(Locale.ROOT)
    return "$hostId|$normalized"
}
