package dev.acportal.presentation

import android.os.Process
import android.app.ActivityManager
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.data.McpDefinition
import dev.acportal.storage.HostProfile
import dev.acportal.storage.StoredSession
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.Properties
import java.security.MessageDigest

/** Run storage preparation, the external task workflow, then verification alone. */
class TaskRecoveryNavigationTest {
    private val context get()=InstrumentationRegistry.getInstrumentation().targetContext
    private val root get()=File(context.cacheDir,"task-recovery-fixture")
    private val definition=McpDefinition("task-server","Task saved server","http","https://task.example.invalid")
    private val scenario get()=InstrumentationRegistry.getArguments().getString("taskScenario") ?: "Mcp"
    private fun offlineSession(index:Int):StoredSession {
        val info=SessionInfo("offline-$index","fixture-agent","offline-acp-$index","/fixture/$index",status="interrupted")
        val state=SessionState(sequence=12,processing=true,items=listOf(
            TimelineItem.Text("user-$index","user","Offline $index prompt"),
            TimelineItem.Text("agent-$index","agent","Offline $index retained answer")
        ),permissions=mapOf("stale-$index" to Permission(JsonPrimitive("stale-$index"),buildJsonObject {
            putJsonObject("toolCall") {put("title","Stale offline approval")}
            putJsonArray("options") {add(buildJsonObject {put("optionId","stale-allow");put("name","Do not approve stale request");put("kind","allow_once")})}
        })))
        return StoredSession("task-host",info.id,WireJson.encodeToString(info),WireJson.encodeToString(state),"Offline $index retained draft",updatedAt=index.toLong())
    }
    private fun liveSession(index:Int):StoredSession {
        val info=SessionInfo("live-$index","fixture-agent","acp-live-$index","/fixture/live-$index",status="running")
        val state=SessionState(sequence=12,processing=index==1,promptRequestId=if(index==1)JsonPrimitive("fixture-turn").toString() else null,items=(0..19).map {line->TimelineItem.Text("live-history-$index-$line","agent","Live $index history $line. "+"Retained fixture history. ".repeat(8))}+listOf(
            TimelineItem.Text("live-user-$index","user","Live $index prompt"),
            TimelineItem.Tool("live-tool-$index","Retained tool $index","execute","completed",content=buildJsonArray {add(buildJsonObject {put("type","content");putJsonObject("content") {put("type","text");put("text","Retained tool output $index")}})}),
            TimelineItem.Text("live-thought-$index","thought","Retained thought $index"),
            TimelineItem.Text("live-agent-$index","agent","Live $index retained answer")
        ))
        return StoredSession("task-host",info.id,WireJson.encodeToString(info),WireJson.encodeToString(state),"Live $index retained draft",updatedAt=index.toLong())
    }
    private fun properties(name:String)=Properties().apply {File(root,name).inputStream().use(::load)}
    private fun fingerprint(records:Iterable<Any>):String {
        val digest=MessageDigest.getInstance("SHA-256")
        records.forEach {digest.update(it.toString().toByteArray(Charsets.UTF_8));digest.update(0.toByte())}
        return digest.digest().joinToString("") {"%02x".format(it)}
    }
    private fun productionSnapshot(repository:dev.acportal.data.PortalRepository)=runBlocking {
        mapOf(
            "productionHosts" to fingerprint(repository.hosts.first().sortedBy {it.id}),
            "productionSessions" to fingerprint(repository.sessions.first().sortedWith(compareBy({it.hostId},{it.id}))),
            "productionUi" to fingerprint(listOf(repository.uiState.first()))
        )
    }
    @Suppress("DEPRECATION")
    private fun removeOwnedTask(taskId:Int) {
        val manager=context.getSystemService(ActivityManager::class.java)
        manager.appTasks.firstOrNull {it.taskInfo.persistentId==taskId}?.let {task->
            assertTrue("Refuse to remove an unrelated task",task.taskInfo.baseIntent.component?.className in setOf("dev.acportal.MainActivity","dev.acportal.presentation.TaskRecoveryFixtureActivity"))
            task.finishAndRemoveTask()
        }
        assertFalse("Recorded fixture task remains",manager.appTasks.any {it.taskInfo.persistentId==taskId})
    }
    @Test fun removeRecordedFixtureTaskAfterInterruptedCleanup() {
        val taskId=InstrumentationRegistry.getArguments().getString("fixtureTaskId")?.toIntOrNull()
        require(taskId!=null) {"Supply only the verified fixture task ID"}
        removeOwnedTask(taskId)
    }
    @Test fun cleanOwnedStorageAfterInterruptedWorkflow() {
        val application=context.applicationContext as TaskRecoveryFixtureApplication
        val actual=application.productionRepository
        val snapshot=properties("storage.properties")
        assertEquals(snapshot.getProperty("productionAliases"),runBlocking {actual.settings.mcpAliases.first().toSortedMap().toString()})
        productionSnapshot(actual).forEach {key,value->assertEquals("Production records changed: $key",snapshot.getProperty(key),value)}
        removeOwnedTask(properties("created.properties").getProperty("task").toInt())
        application.closeFixtureStore()
        assertSame(actual,application.repository)
        assertTrue(root.deleteRecursively())
    }
    @Test fun prepareOwnedStorageForExternalTaskWorkflow() {
        assertFalse("Verify or clean the previous owned fixture first",root.exists())
        assertTrue(root.mkdirs());File(root,"enabled").writeText("")
        val actual=(context.applicationContext as TaskRecoveryFixtureApplication).productionRepository
        val aliases=runBlocking {actual.settings.mcpAliases.first()}
        val snapshot=productionSnapshot(actual)
        val store=TaskRecoveryFixtureStore(context)
        var prepared=false
        try {
            runBlocking {
                val address=if(scenario=="LiveConversation") {
                    val port=InstrumentationRegistry.getArguments().getString("fixturePort")?.toIntOrNull()
                    require(port!=null && port in 1024..65535) {"Supply the owned reversed fixture port"}
                    "http://127.0.0.1:$port"
                } else "http://127.0.0.1:1"
                val credential=if(scenario=="LiveConversation")store.vault.save("recovery-fixture-only") else "unused"
                store.db.portal().saveHost(HostProfile("task-host","Task workstation",address,credential,"fixture",Long.MAX_VALUE))
                store.repository.saveMcpServers("task-host",listOf(definition))
                if(scenario=="OfflineConversation") {
                    (1..2).forEach {store.db.portal().saveSession(offlineSession(it))}
                    store.db.portal().saveMainPage("sessions")
                } else if(scenario=="LiveConversation") {
                    (1..2).forEach {store.db.portal().saveSession(liveSession(it))}
                    store.db.portal().saveMainPage("sessions")
                } else require(scenario=="Mcp") {"Unknown task scenario"}
            }
            Properties().apply {
                setProperty("alias",runBlocking {store.settings.mcpAliases.first().getValue("task-host")})
                setProperty("scenario",scenario)
                setProperty("productionAliases",aliases.toSortedMap().toString())
                snapshot.forEach {key,value->setProperty(key,value)}
                File(root,"storage.properties").outputStream().use {store(it,"Fixture alias and production preservation checkpoint")}
            }
            assertEquals(aliases,runBlocking {actual.settings.mcpAliases.first()})
            assertEquals(snapshot,productionSnapshot(actual))
            prepared=true
        } finally {store.close();if(!prepared)root.deleteRecursively()}
    }
    @Test fun verifyRecordedTaskRestorationAndCleanOwnedStorage() {
        val previous=properties("checkpoint.properties")
        val restored=properties("created.properties")
        assertNotEquals(previous.getProperty("pid"),restored.getProperty("pid"))
        assertNotEquals(previous.getProperty("pid").toInt(),Process.myPid())
        assertEquals(previous.getProperty("task"),restored.getProperty("task"))
        assertEquals("true",restored.getProperty("savedState"))
        val application=context.applicationContext as TaskRecoveryFixtureApplication
        val store=application.fixtureStore ?: TaskRecoveryFixtureStore(context)
        try {
            assertEquals(listOf(definition),runBlocking {store.repository.mcpServers("task-host")})
            assertEquals(properties("storage.properties").getProperty("alias"),runBlocking {store.settings.mcpAliases.first().getValue("task-host")})
            if(properties("storage.properties").getProperty("scenario")=="OfflineConversation") {
                runBlocking {(1..2).forEach {index->assertEquals("Offline history/draft changed",offlineSession(index),store.db.portal().session("task-host","offline-$index"))}}
            }
            if(properties("storage.properties").getProperty("scenario")=="LiveConversation") {
                runBlocking {(1..2).forEach {index->
                    val saved=store.db.portal().session("task-host","live-$index")!!
                    assertEquals(liveSession(index).draft,saved.draft)
                    val state=WireJson.decodeFromString<SessionState>(saved.state)
                    assertEquals(WireJson.decodeFromString<SessionState>(liveSession(index).state).items,state.items)
                    val overflow=InstrumentationRegistry.getArguments().getString("fixtureOverflow") ?: "None"
                    if(index==2 && overflow!="None") {
                        assertTrue(state.replaying)
                        assertTrue("Live approvals must not enter disk history",state.permissions.isEmpty())
                    } else assertFalse(state.replaying)
                    assertTrue("Stale permission persisted after authoritative replay",state.permissions.values.none {it.id.text().startsWith("old-")})
                }}
            }
            val actual=application.productionRepository
            assertEquals(properties("storage.properties").getProperty("productionAliases"),runBlocking {actual.settings.mcpAliases.first().toSortedMap().toString()})
            productionSnapshot(actual).forEach {key,value->assertEquals("Production records changed: $key",properties("storage.properties").getProperty(key),value)}
        } finally {
            removeOwnedTask(restored.getProperty("task").toInt())
            if(application.fixtureStore===store)application.closeFixtureStore() else store.close()
            assertNull(application.fixtureStore)
            assertSame(application.productionRepository,application.repository)
            assertTrue(root.deleteRecursively())
        }
    }
}
