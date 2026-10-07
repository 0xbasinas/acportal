package dev.acportal.data

import dev.acportal.protocol.*
import dev.acportal.storage.*
import org.junit.Assert.*
import org.junit.Test

class WorkspaceRecentsTest {
    private val host=HostProfile("h","Host","https://host.example","alias","d",0)
    @Test fun currentRootsRejectTraversalSiblingPrefixesAndOtherDrives() {
        val roots=listOf(Workspace("C:\\Projects","Projects"))
        assertTrue(workspaceInCurrentRoots("h","\\\\?\\C:\\projects\\app",roots))
        assertFalse(workspaceInCurrentRoots("h","C:\\Projects-other\\app",roots))
        assertFalse(workspaceInCurrentRoots("h","C:\\Projects\\app\\..\\..\\outside",roots))
        assertFalse(workspaceInCurrentRoots("h","D:\\Projects\\app",roots))
        assertTrue(workspaceInCurrentRoots("h","/app",listOf(Workspace("/","Root"))))
    }
    @Test fun recentsPreserveAvailableCombinationsAndExcludeStaleEntries() {
        val entries=listOf(RecentWorkspace("h","/projects/a","one",1),RecentWorkspace("h","/projects/a","one",5),RecentWorkspace("h","/projects/a","two",4),RecentWorkspace("other","/projects/b","one",8),RecentWorkspace("h","/outside/a","one",9),RecentWorkspace("h","/projects/c","missing",10))
        val details=HostDetails(host,listOf(AgentInfo("one","One",installed=true),AgentInfo("two","Two",installed=true),AgentInfo("missing","Missing")),listOf(Workspace("/projects","Projects")),emptyList(),entries)
        val choices=recentWorkspaceChoices(details)
        assertEquals(listOf("one","two"),choices.map {it.agentId});assertEquals(5,choices.first().lastUsed)
        assertEquals(listOf("two"),recentWorkspaceChoices(details,"two").map {it.agentId})
        assertTrue(recentWorkspaceChoices(details.copy(agents=details.agents.map {it.copy(enabled=false)})).isEmpty())
    }
}
