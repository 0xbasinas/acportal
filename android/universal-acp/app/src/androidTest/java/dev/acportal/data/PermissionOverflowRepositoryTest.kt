package dev.acportal.data

import android.content.ContextWrapper
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import dev.acportal.transport.ConnectionState
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

class PermissionOverflowRepositoryTest {
    @Test fun actualSocketOverflowDetachesAndCannotReenableDecisionsFromQueuedReplay() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val root = File(context.cacheDir, "permission-overflow-${UUID.randomUUID()}").apply { mkdirs() }
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val database = Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val vault = CredentialVault(object : ContextWrapper(context) { override fun getNoBackupFilesDir() = root })
        val settings = SettingsStore(context, PreferenceDataStoreFactory.create(scope = scope, produceFile = { File(root,"settings.preferences_pb") }))
        val client = HostApi.client()
        val server = MockWebServer()
        val decisions = AtomicInteger()
        val peer = CompletableDeferred<WebSocket>()
        val info = SessionInfo("session", "fixture", "acp", "/fixture")
        server.enqueue(MockResponse().setBody(WireJson.encodeToString(SessionInfo.serializer(), info)))
        server.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) { peer.complete(webSocket) }
            override fun onMessage(webSocket: WebSocket, text: String) { decisions.incrementAndGet() }
        }))
        server.start()
        val repository = PortalRepository(database.portal(), HostApi(client,vault), vault, settings, scope)
        var live: LiveSession? = null
        try {
            val alias = vault.save("isolated-fixture-credential")
            database.portal().saveHost(HostProfile("host", "Fixture", server.url("/").toString(), alias, "device", Long.MAX_VALUE))
            database.portal().saveSession(StoredSession("host", info.id, WireJson.encodeToString(SessionInfo.serializer(),info)))
            val session = repository.open("host",info.id)
            live = session
            val socket = withTimeout(10000) { peer.await() }
            assertTrue(socket.send("""{"type":"replay_start"}"""))
            assertTrue(socket.send("""{"type":"replay_complete","latestSequence":0}"""))
            withTimeout(10000) { session.state.first { !it.replaying } }
            repeat(129) { index ->
                assertTrue(socket.send(buildJsonObject {
                    put("type","event"); put("sequence", index+1); put("direction","agent")
                    putJsonObject("message") { put("id","p-$index"); put("method","session/request_permission")
                        putJsonObject("params") { putJsonArray("options") { add(buildJsonObject { put("optionId","allow"); put("kind","allow_once") }) } }
                    }
                }.toString()))
            }
            // A replay completion queued behind overflow must not enable the retained decisions.
            socket.send("""{"type":"replay_complete","latestSequence":129}""")
            withTimeout(10000) { session.state.first { it.errorOrigin == SessionErrorOrigin.PROTOCOL && it.error?.contains("client limit") == true } }
            withTimeout(10000) { session.connection.first { it == ConnectionState.Disconnected } }
            delay(300)
            assertTrue(session.manuallyDetached)
            assertTrue(session.state.value.replaying)
            assertEquals(128, session.state.value.permissions.size)
            assertEquals(128L, session.state.value.sequence)
            val consent = session.state.value.permissions.values.first()
            try { repository.permission(session,consent,"allow"); fail("Detached consent must not send") } catch (_: IllegalStateException) {}
            repository.dismissSessionError(session)
            assertTrue(session.manuallyDetached)
            try { repository.prompt(session,"do not submit"); fail("Detached prompt must not send") } catch (_: IllegalStateException) {}
            assertEquals(0,decisions.get())
            assertEquals(2,server.requestCount)
        } finally {
            live?.close()
            scope.coroutineContext[Job]?.cancelAndJoin()
            database.close()
            server.shutdown()
            client.dispatcher.executorService.shutdown()
            client.connectionPool.evictAll()
            assertTrue(root.deleteRecursively())
        }
    }
}
