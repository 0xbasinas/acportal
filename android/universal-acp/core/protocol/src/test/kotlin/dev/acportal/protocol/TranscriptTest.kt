package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class TranscriptTest {
    private val info=SessionInfo("id","custom-agent","session","/projects/app",initialization=buildJsonObject {put("secret","must-not-export")})
    @Test fun readableSnapshotIncludesDiffAndTrustedTerminalOutputWithoutRawState() {
        val state=SessionState(items=listOf(TimelineItem.Text("u","user","Please inspect\nthis file"),TimelineItem.Tool("t","Write example.kt","edit","completed",WireJson.parseToJsonElement("""[{"type":"diff","path":"example.kt","oldText":"old","newText":"new"},{"type":"terminal","terminalId":"terminal"}]""").arrayValue())),terminals=mapOf("terminal" to buildJsonObject {put("output","literal ``` code\nverified");put("truncated",true);putJsonObject("exitStatus") {put("exitCode",0)}}),permissions=mapOf("p" to Permission(JsonPrimitive("p"),buildJsonObject {put("secret","pending-decision-secret")})))
        val text=transcriptMarkdown(info,state)
        assertTrue(text.contains("## You\n\nPlease inspect\nthis file"))
        assertTrue(text.contains("Before:\n\n```\nold"))
        assertTrue(text.contains("After:\n\n```\nnew"))
        assertTrue(text.contains("````\nliteral ``` code\nverified\n````"))
        assertTrue(text.contains("Earlier terminal output was omitted"))
        assertTrue(text.contains("Exit code: 0"))
        assertFalse(text.contains("must-not-export"))
        assertFalse(text.contains("pending-decision-secret"))
    }
    @Test fun partialWorkingAndEmptySnapshotsDoNotClaimCompleteHistory() {
        val text=transcriptMarkdown(info,SessionState(localHistoryCleared=true,processing=true))
        assertTrue(text.contains("Earlier messages are not included"))
        assertTrue(text.contains("session was still working"))
        assertTrue(text.contains("No messages available"))
    }
    @Test fun unknownTerminalAndMarkdownInTitlesAreSafeAndReadable() {
        val text=transcriptMarkdown(info,SessionState(items=listOf(TimelineItem.Tool("t","# title\nsecond line",content=buildJsonArray {add(buildJsonObject {put("type","terminal");put("terminalId","gone")})}))))
        assertTrue(text.contains("## \\# title second line"))
        assertTrue(text.contains("Terminal output is no longer available"))
    }
    @Test fun exportIsBoundedByUtf8Bytes() {
        assertThrows(IllegalArgumentException::class.java) {transcriptMarkdown(info,SessionState(items=listOf(TimelineItem.Text("t","agent","界".repeat(3*1024*1024)))))}
    }
}
