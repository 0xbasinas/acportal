package dev.acportal.data

import android.content.ContextWrapper
import android.os.Debug
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.presentation.SessionListSummaries
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import okhttp3.*
import okhttp3.mockwebserver.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.lang.ref.WeakReference
import java.util.UUID
import java.util.concurrent.ConcurrentHashMap

object NativeAllocationProbe {external fun report(path:String,finish:Boolean):String}

/** Opt-in class/symbol-only profiler. No heap dump or object-value capture. */
class AllocationSitesLifecycleProfileTest {
    @Test fun actualRepositoryStreamingAndDeletionReleaseLiveSessions()=runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(InstrumentationRegistry.getArguments().getString("allocationSites")=="true")
        require(context.packageName=="dev.acportal.acceptance")
        val library=File(context.cacheDir,"allocation-profile/libacportalalloc.so")
        require(library.isFile)
        System.load(library.absolutePath)
        Debug.attachJvmtiAgent(library.absolutePath,null,NativeAllocationProbe::class.java.classLoader)
        val output=library.parentFile!!
        val root=File(context.cacheDir,"allocation-store-${UUID.randomUUID()}").apply {mkdirs()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir()=root})
        val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={File(root,"settings.preferences_pb")}))
        val client=HostApi.client()
        val peers=ConcurrentHashMap<String,WebSocket>()
        val server=MockWebServer()
        server.dispatcher=object:okhttp3.mockwebserver.Dispatcher() {
            override fun dispatch(request:RecordedRequest):MockResponse {
                val id=request.path!!.substringAfter("/v1/sessions/").substringBefore('/')
                if(request.method=="DELETE")return MockResponse().setResponseCode(204)
                if(request.path!!.contains("/connect"))return MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
                    override fun onOpen(webSocket:WebSocket,response:Response) {peers[id]=webSocket;webSocket.send("""{"type":"replay_start"}""");webSocket.send("""{"type":"replay_complete","latestSequence":0}""")}
                })
                return MockResponse().setBody(WireJson.encodeToString(SessionInfo.serializer(),SessionInfo(id,"fixture","acp-$id","/fixture")))
            }
        }
        server.start()
        val repository=PortalRepository(database.portal(),HostApi(client,vault),vault,settings,scope)
        try {
            database.portal().saveHost(HostProfile("allocation-host","Fixture",server.url("/").toString(),vault.save("allocation-only"),"fixture",Long.MAX_VALUE))
            val refs=exercise(repository,database,peers,output)
            // The producer frame has returned; only the repository intentionally owns
            // the live objects here. No test variable retains their strong references.
            val open=NativeAllocationProbe.report(File(output,"open.tsv").absolutePath,false)
            assertTrue(open,open.contains("liveSessions=2"))
            assertTrue(refs.all {it.get()!=null})
            database.portal().sessions().first().forEach {repository.delete(it)}
            peers.clear()
            delay(1000)
            val closed=NativeAllocationProbe.report(File(output,"closed.tsv").absolutePath,true)
            assertTrue(closed,closed.contains("liveSessions=0"))
            assertTrue("Deleted live sessions remain strongly reachable",refs.all {it.get()==null})
            val symbols=File(output,"closed.tsv").readText()
            assertTrue("No application allocation stacks collected",symbols.contains("Ldev/acportal/"))
            assertTrue("No direct owner attribution collected",File(output,"open.tsv").readText().contains("directOwner\tLdev/acportal/data/LiveSession;"))
            assertTrue(database.portal().sessions().first().isEmpty())
        } finally {
            scope.coroutineContext[Job]?.cancelAndJoin();database.close();server.shutdown()
            client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()
            assertTrue(root.deleteRecursively())
        }
    }
    private suspend fun exercise(repository:PortalRepository,database:PortalDatabase,peers:ConcurrentHashMap<String,WebSocket>,output:File):List<WeakReference<LiveSession>> {
        val refs=mutableListOf<WeakReference<LiveSession>>()
        for(index in 1..2) {
            val info=SessionInfo("allocation-$index","fixture","acp-allocation-$index","/fixture")
            database.portal().saveSession(StoredSession("allocation-host",info.id,WireJson.encodeToString(SessionInfo.serializer(),info)))
            val live=repository.open("allocation-host",info.id)
            withTimeout(10_000) {live.state.first {!it.replaying && peers.containsKey(info.id)}}
            refs.add(WeakReference(live))
            for(sequence in 1..100) {
                peers.getValue(info.id).send(buildJsonObject {put("type","event");put("sequence",sequence);put("direction","agent");putJsonObject("message") {
                    put("jsonrpc","2.0");put("method","session/update");putJsonObject("params") {
                        put("sessionId",info.acpSessionId);putJsonObject("update") {put("sessionUpdate","agent_message_chunk");putJsonObject("content") {put("type","text");put("text","Bounded streaming fixture. ".repeat(8))}}
                    }
                }}.toString())
            }
            withTimeout(15_000) {live.state.first {it.sequence==100L}}
        }
        delay(700)
        val summaries=SessionListSummaries()
        repeat(20) {summaries.update(database.portal().sessions().first())}
        assertEquals(2,summaries.decoded)
        File(output,"summary-count.txt").writeText("unchangedRefreshes=20;decodedRows=2\n")
        return refs
    }
}
