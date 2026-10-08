package dev.acportal.presentation

import dev.acportal.protocol.AgentInfo
import org.junit.Assert.assertEquals
import org.junit.Test

class AgentStatusTest {
    @Test fun ownToolsIsAddedOnlyWhereAskedAndOnlyForFlaggedAgents() {
        val agent=AgentInfo("opencode","OpenCode",installed=true,status="available",ownTools=true)
        assertEquals("Available",registryStatus(agent))
        assertEquals("Available · Uses its own tools",registryStatus(agent,withTools=true))
        assertEquals("Disabled · Uses its own tools",registryStatus(agent.copy(enabled=false),withTools=true))
        assertEquals("Running",registryStatus(agent.copy(status="running",ownTools=false),withTools=true))
    }
}
