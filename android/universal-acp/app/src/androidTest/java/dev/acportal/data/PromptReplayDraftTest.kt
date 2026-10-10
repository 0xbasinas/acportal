package dev.acportal.data

import android.content.ContextWrapper
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import okhttp3.*
import okhttp3.mockwebserver.*
import org.junit.Assert.*
import org.junit.Test
import java.io.File
import java.util.UUID
import java.util.concurrent.atomic.AtomicInteger

class PromptReplayDraftTest {
    @Test fun historicalPromptReplayPreservesDraftButCurrentSubmissionAcknowledgementClearsIt() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "draft-replay-${UUID.randomUUID()}").apply { mkdirs() }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val database = Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val vault = CredentialVault(object : ContextWrapper(context) { override fun getNoBackupFilesDir() = root })
        val settings = SettingsStore(context, PreferenceDataStoreFactory.create(scope=scope, produceFile={File(root,"settings.preferences_pb")}))
        val client = HostApi.client()
        val server = MockWebServer()
        val peer = CompletableDeferred<WebSocket>()
        val submission = CompletableDeferred<String>()
        val sends = AtomicInteger()
        val info = SessionInfo("session", "fixture", "acp", "/fixture")
        server.enqueue(MockResponse().setBody(WireJson.encodeToString(SessionInfo.serializer(), info)))
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {peer.complete(webSocket)}
            override fun onMessage(webSocket:WebSocket,text:String) {sends.incrementAndGet();submission.complete(text)}
        }))
        server.start()
        val repository = PortalRepository(database.portal(),HostApi(client,vault),vault,settings,scope)
        var live:LiveSession? = null
        try {
            database.portal().saveHost(HostProfile("host","Fixture",server.url("/").toString(),vault.save("isolated-fixture"),"device",Long.MAX_VALUE))
            database.portal().saveSession(StoredSession("host",info.id,WireJson.encodeToString(SessionInfo.serializer(),info),draft="Unsent after earlier turn"))
            val session=repository.open("host",info.id);live=session
            val socket=withTimeout(10000) {peer.await()}
            socket.send("""{"type":"replay_start"}""")
            socket.send("""{"type":"event","sequence":1,"direction":"client","message":{"jsonrpc":"2.0","id":"historical","method":"session/prompt","params":{"sessionId":"acp","prompt":[{"type":"text","text":"Earlier prompt"}]}}}""")
            socket.send("""{"type":"event","sequence":2,"direction":"agent","message":{"jsonrpc":"2.0","id":"historical","result":{"stopReason":"end_turn"}}}""")
            socket.send("""{"type":"replay_complete","latestSequence":2}""")
            withTimeout(10000) {session.state.first {!it.replaying && !it.processing && it.sequence==2L}}
            assertEquals("Unsent after earlier turn",database.portal().session("host",info.id)!!.draft)
            assertEquals(0,sends.get())
            repository.prompt(session,"Explicit new fixture prompt")
            val wire=withTimeout(10000) {submission.await()}
            socket.send(buildJsonObject {put("type","event");put("sequence",3);put("direction","client");put("message",WireJson.parseToJsonElement(wire))}.toString())
            withTimeout(10000) {database.portal().sessions().first {it.single().draft.isEmpty()}}
            assertEquals(1,sends.get())
        } finally {
            live?.close();scope.coroutineContext[Job]?.cancelAndJoin();database.close();server.shutdown()
            client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()
            assertTrue(root.deleteRecursively())
        }
    }
}
