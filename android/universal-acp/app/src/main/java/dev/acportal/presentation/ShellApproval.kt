package dev.acportal.presentation

import dev.acportal.protocol.*

/**
 * A host consent for one shell line. The phone must show [line] in full, because
 * acpd runs exactly this text through [shell] once the user approves it.
 */
data class ShellCommandApproval(val line:String,val shell:String,val cwd:String,val environmentNames:List<String>,val autoReviewReason:String?=null)

const val SHELL_COMMAND_SOURCE="host-shell-command"

fun shellCommandApproval(permission:Permission):ShellCommandApproval? {
    if(permission.request["_meta"].objectValue()["acpdSource"].text()!=SHELL_COMMAND_SOURCE)return null
    val input=permission.request["toolCall"].objectValue()["rawInput"].objectValue()
    val line=input["shellLine"].text()
    if(line.isEmpty())return null
    val review=permission.request["_meta"].objectValue()["acpdAutoReview"].objectValue()
    val reason=review["reason"].text().takeIf {it.isNotBlank()}?.let {"${review["layer"].text().ifBlank {"auto"}} review: $it"}
    return ShellCommandApproval(line,input["shell"].text().ifBlank {"the host shell"},input["cwd"].text(),input["environmentNames"].arrayValue().map {it.text()}.filter {it.isNotBlank()},reason)
}

/** Summary shown above the full line, e.g. "Runs with /bin/sh -c in /work/app". */
fun ShellCommandApproval.runsWith():String = if(cwd.isBlank())"Runs with $shell" else "Runs with $shell in $cwd"
