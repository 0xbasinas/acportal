package dev.acportal.presentation

import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ShellApprovalTest {
    private fun permission(source:String,rawInput:String)=Permission(JsonPrimitive(7),WireJson.parseToJsonElement("""{"_meta":{"acpdSource":"$source"},"toolCall":{"title":"Run shell command","rawInput":$rawInput},"options":[{"optionId":"acpd-shell-deny","name":"Deny","kind":"reject_once"},{"optionId":"acpd-shell-allow","name":"Run this shell line once","kind":"allow_once"}]}""").objectValue())

    @Test fun shellConsentKeepsTheWholeLineVerbatim() {
        val line="cd /w && python3 -m unittest -v 2>&1; echo \"done\" | tee log.txt "+"x".repeat(5000)
        val raw=buildJsonObject {put("shellLine",line);put("shell","/bin/sh -c");put("cwd","/w");putJsonArray("environmentNames") {add("AGENT_SESSION_ID");add("")}}
        val approval=shellCommandApproval(permission(SHELL_COMMAND_SOURCE,raw.toString()))!!
        assertEquals(line,approval.line)
        assertEquals(line.length,approval.line.length)
        assertEquals(listOf("AGENT_SESSION_ID"),approval.environmentNames)
        assertEquals("Runs with /bin/sh -c in /w",approval.runsWith())
    }
    @Test fun onlyHostShellConsentsAreShellApprovals() {
        assertNull(shellCommandApproval(permission("host-terminal","""{"command":"/usr/bin/ls","args":["-la"]}""")))
        assertNull(shellCommandApproval(permission(SHELL_COMMAND_SOURCE,"""{"shell":"/bin/sh -c"}""")))
        assertNull(shellCommandApproval(Permission(JsonPrimitive(1),WireJson.parseToJsonElement("""{"toolCall":{"rawInput":{"shellLine":"rm -rf x"}}}""").objectValue())))
    }
    @Test fun missingShellAndCwdStillDescribeTheRun() {
        val approval=shellCommandApproval(permission(SHELL_COMMAND_SOURCE,"""{"shellLine":"ls -la"}"""))!!
        assertEquals("Runs with the host shell",approval.runsWith())
        assertTrue(approval.environmentNames.isEmpty())
    }
}
