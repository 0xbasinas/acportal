package dev.acportal.protocol

import org.junit.Test
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue

class AgentGroupsTest {
    private fun agent(id: String, name: String = id, profileOf: String? = null, label: String? = null) =
        AgentInfo(id, name, profileOf = profileOf, profileLabel = label)

    @Test fun profilesFollowTheirBaseAndNothingIsDropped() {
        val list = listOf(
            agent("goose", "Goose"),
            agent("mock", "Mock"),
            agent("goose-work", "Goose work", profileOf = "goose", label = "Work"),
            agent("orphan", "Orphan", profileOf = "gone", label = "Lost"),
        )
        assertEquals(listOf("goose", "goose-work", "mock", "orphan"), orderedByProfile(list).map { it.id })
        assertEquals(list.size, orderedByProfile(list).size)
    }

    @Test fun aChainIsKeptListedOnce() {
        val list = listOf(
            agent("a", "A"),
            agent("b", "B", profileOf = "a"),
            agent("c", "C", profileOf = "b"),
        )
        val ordered = orderedByProfile(list).map { it.id }
        assertEquals(setOf("a", "b", "c"), ordered.toSet())
        assertEquals(3, ordered.size)
    }

    @Test fun titlesNameTheBaseOnlyWhenItIsListed() {
        val base = agent("goose", "Goose")
        val work = agent("goose-work", "Goose work", profileOf = "goose", label = "Work")
        val orphan = agent("orphan", "Orphan", profileOf = "gone", label = "Lost")
        assertEquals("Goose · Work", agentTitle(work, listOf(base, work)))
        assertEquals("Orphan", agentTitle(orphan, listOf(base, orphan)))
        assertEquals("Goose", agentTitle(base, listOf(base, work)))
        assertEquals("Goose · Goose work", agentTitle(work.copy(profileLabel = null), listOf(base, work)))
    }

    @Test fun theHostFieldsDecodeAndOlderHostsStillDo() {
        val withProfile = WireJson.decodeFromString<AgentInfo>("""{"id":"w","name":"W","profileOf":"goose","profileLabel":"Work"}""")
        assertEquals("goose", withProfile.profileOf)
        assertEquals("Work", withProfile.profileLabel)
        val old = WireJson.decodeFromString<AgentInfo>("""{"id":"w","name":"W"}""")
        assertNull(old.profileOf)
        val access = WireJson.decodeFromString<WorkspaceAccess>("""{"readFiles":true,"writeFiles":true,"terminal":true,"manualApproval":true}""")
        assertTrue(access.manualApproval)
        assertFalse(WireJson.decodeFromString<WorkspaceAccess>("""{"readFiles":true,"writeFiles":true,"terminal":true}""").manualApproval)
    }
}
