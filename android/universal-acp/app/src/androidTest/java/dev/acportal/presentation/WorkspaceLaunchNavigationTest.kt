package dev.acportal.presentation

import android.database.sqlite.SQLiteDatabase
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.lifecycle.ViewModelProvider
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.*
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class WorkspaceLaunchNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()

    @Test fun removedRecentFolderFailsWithoutMutationAndExistingRecentLaunchesExplicitly() {
        val repository=(compose.activity.application as PortalApplication).repository
        val host=runBlocking {repository.hosts.first().firstOrNull()}
        val removed=InstrumentationRegistry.getArguments().getString("removedWorkspace")
        assumeTrue("Requires the paired mock host and a removed host-folder fixture",host!=null && removed!=null)
        val original=host!!
        val root=runBlocking {repository.api.workspaces(original).first().path}
        val database=PortalDatabase.open(compose.activity)
        val dao=database.portal()
        val previousUi=runBlocking {repository.uiState.first()}
        // Read every row, including older entries outside the normal 20-row display limit.
        val previousRecents=SQLiteDatabase.openDatabase(compose.activity.getDatabasePath("portal.db").path,null,SQLiteDatabase.OPEN_READONLY).use {db->
            db.rawQuery("SELECT hostId,path,agentId,lastUsed FROM recent_workspaces WHERE hostId=?",arrayOf(original.id)).use {cursor->
                buildList {while(cursor.moveToNext())add(RecentWorkspace(cursor.getString(0),cursor.getString(1),cursor.getString(2),cursor.getLong(3)))}
            }
        }
        val beforeHost=runBlocking {repository.api.sessions(original).map {it.id}.toSet()}
        val beforeLocal=runBlocking {repository.sessions.first().map {it.hostId to it.id}.toSet()}
        lateinit var vm:PortalViewModel
        compose.runOnIdle {vm=ViewModelProvider(compose.activity)[PortalViewModel::class.java]}
        compose.waitUntil(20_000) {!vm.ui.value.loading}
        try {
            runBlocking {
                dao.saveRecent(RecentWorkspace(original.id,root,"mock",System.currentTimeMillis()))
                dao.saveRecent(RecentWorkspace(original.id,removed!!,"mock",System.currentTimeMillis()+1))
            }
            val seeded=runBlocking {dao.recent(original.id)}
            compose.onNodeWithContentDescription("Connections tab").performClick()
            compose.onNodeWithText(original.label).performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("New session").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            compose.onNodeWithText("New session").performClick()
            compose.onNodeWithTag("new-session-choices").performScrollToNode(hasTestTag("recent:mock:$removed"))
            compose.onNodeWithTag("recent:mock:$removed").performClick()
            compose.onNodeWithText("Start session").performClick()
            compose.waitUntil(20_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
            assertTrue(vm.ui.value.error!!.contains("existing allowed folder"))
            compose.onNodeWithText(vm.ui.value.error!!).assertIsDisplayed()
            assertEquals(beforeHost,runBlocking {repository.api.sessions(original).map {it.id}.toSet()})
            assertEquals(beforeLocal,runBlocking {repository.sessions.first().map {it.hostId to it.id}.toSet()})
            assertEquals(seeded,runBlocking {dao.recent(original.id)})
            // Changing a choice must not retry the failed creation.
            compose.onNodeWithContentDescription("Dismiss error").performClick()
            compose.onNodeWithTag("new-session-choices").performScrollToNode(hasTestTag("recent:mock:$root"))
            compose.onNodeWithTag("recent:mock:$root").performClick()
            assertEquals(beforeHost,runBlocking {repository.api.sessions(original).map {it.id}.toSet()})
            compose.onNodeWithText("Start session").performClick()
            compose.waitUntil(30_000) {vm.ui.value.live?.let {it.info.id !in beforeHost && it.connection.value==ConnectionState.Connected && !it.state.value.replaying}==true}
            val live=vm.ui.value.live!!
            assertEquals("mock",live.info.agentId)
            assertEquals(root,live.info.workspace)
            assertFalse(live.state.value.processing)
            assertEquals(beforeHost+live.info.id,runBlocking {repository.api.sessions(original).map {it.id}.toSet()})
        } finally {
            runBlocking {
                repository.sessions.first().filter {it.hostId==original.id && (it.hostId to it.id) !in beforeLocal}.forEach {repository.delete(it)}
                dao.deleteRecent(original.id)
                previousRecents.forEach {dao.saveRecent(it)}
                repository.saveMainPage(previousUi.mainPage)
                repository.saveSessionFilters(previousUi.sessionsArchived,previousUi.sessionsActive,previousUi.sessionsQuery)
            }
            database.close()
        }
    }
}
