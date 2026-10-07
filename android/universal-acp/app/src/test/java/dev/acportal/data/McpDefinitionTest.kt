package dev.acportal.data

import dev.acportal.protocol.*
import org.junit.Assert.*
import org.junit.Test

class McpDefinitionTest {
    @Test fun stdioUsesLiteralArgumentsAndRequiredArrays() {
        val definition=McpDefinition("id","tools",endpoint="C:\\tools\\server.exe",args=listOf("argument with spaces",""),values=listOf(McpValue("KEY","a=b")))
        val wire=definition.wire()
        assertNull(wire["type"])
        assertEquals("argument with spaces",wire["args"].arrayValue()[0].text())
        assertEquals("",wire["args"].arrayValue()[1].text())
        assertEquals("a=b",wire["env"].arrayValue()[0].objectValue()["value"].text())
    }
    @Test fun validationRejectsRelativeCommandsInjectedHeadersAndCredentialsInUrls() {
        listOf(
            McpDefinition("id","tools",endpoint="server"),
            McpDefinition("id","docs","http","https://example.com",values=listOf(McpValue("Authorization","secret\r\nInjected: x"))),
            McpDefinition("id","docs","http","https://user:secret@example.com"),
        ).forEach {definition->assertThrows(IllegalArgumentException::class.java) {definition.wire()}}
    }
    @Test fun disabledDefinitionsAreOmittedAndDisplayHidesUrlQueries() {
        val definition=McpDefinition("id","docs","sse","https://example.com/events?token=synthetic",enabled=false)
        assertEquals(0,mcpWire(listOf(definition)).size)
        assertEquals("example.com/events",definition.displayEndpoint())
        assertEquals("https://example.com/events?token=synthetic",definition.wire()["url"].text())
    }
    @Test fun valueParsingPreservesEqualsAndSpacesAndEnforcesUniqueNames() {
        assertEquals(listOf(McpValue("KEY"," space=a=b")),parseMcpValues("KEY= space=a=b"))
        val definition=McpDefinition("id","tools",endpoint="/usr/bin/tool",values=listOf(McpValue("KEY","a"),McpValue("key","b")))
        assertThrows(IllegalArgumentException::class.java) {definition.wire()}
        assertThrows(IllegalArgumentException::class.java) {parseMcpValues("invalid-line")}
    }
    @Test fun combinedPayloadAndCountAreBounded() {
        val definition=McpDefinition("id","docs","http","https://example.com",values=listOf(McpValue("A","x".repeat(4096)),McpValue("B","x".repeat(4096)),McpValue("C","x".repeat(4096))))
        assertThrows(IllegalArgumentException::class.java) {mcpWire(listOf(definition))}
        assertThrows(IllegalArgumentException::class.java) {mcpWire(List(17) {McpDefinition("$it","docs$it","http","https://example.com")})}
    }
}
