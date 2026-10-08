package dev.acportal.transport

import dev.acportal.data.validateAddress
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.TimeUnit

class WebSocketTransportTest {
    @Test fun incomingByteOverflowReconnectsWithoutDiscardingAcceptedFramesOrSendingPrompts()=runBlocking {
        val server=MockWebServer();val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO);val client=OkHttpClient()
        val restored=CompletableDeferred<WebSocket>()
        val submissions=java.util.concurrent.atomic.AtomicInteger()
        val frame="x".repeat(1024*1024)
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {repeat(9) {assertTrue(webSocket.send(frame))}}
            override fun onMessage(webSocket:WebSocket,text:String) {submissions.incrementAndGet()}
        }))
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {restored.complete(webSocket)}
            override fun onMessage(webSocket:WebSocket,text:String) {submissions.incrementAndGet()}
        }))
        server.start()
        val transport=WebSocketTransport(client,server.url("/").toString(),"fixture","s",{42},scope)
        try {
            transport.connect()
            val first=withContext(Dispatchers.IO) {server.takeRequest(10,TimeUnit.SECONDS)}!!
            val second=withContext(Dispatchers.IO) {server.takeRequest(10,TimeUnit.SECONDS)}!!
            assertTrue(first.path!!.endsWith("after=42"));assertTrue(second.path!!.endsWith("after=42"))
            val peer=withTimeout(5000) {restored.await()}
            repeat(8) {assertEquals(frame,withTimeout(5000) {transport.messages().first().toString(Charsets.UTF_8)})}
            assertTrue(peer.send("restored"))
            assertEquals("restored",withTimeout(5000) {transport.messages().first().toString(Charsets.UTF_8)})
            assertEquals(0,first.bodySize);assertEquals(0,second.bodySize)
            assertEquals(0,submissions.get())
        } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
    }
    @Test fun terminalFailuresOfferSpecificRecoveryAndStopAutomaticRequests()=runBlocking {
        listOf(403 to ConnectionFailure.AUTHORIZATION,404 to ConnectionFailure.SESSION_UNAVAILABLE,409 to ConnectionFailure.CONTROLLER_BUSY).forEach {(code,kind)->
            val server=MockWebServer();server.enqueue(MockResponse().setResponseCode(code));server.start()
            val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO);val client=OkHttpClient()
            val transport=WebSocketTransport(client,server.url("/").toString(),"credential","s",{7},scope)
            try {
                transport.connect()
                val failed=withTimeout(5000) {transport.connectionState().first {it is ConnectionState.Failed}} as ConnectionState.Failed
                assertEquals(kind,failed.kind)
                assertThrows(IllegalStateException::class.java) {runBlocking {transport.send("a pending prompt".toByteArray())}}
                delay(1100)
                assertEquals(1,server.requestCount)
                assertEquals(0,server.takeRequest(5,TimeUnit.SECONDS)!!.bodySize)
            } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
        }
        assertNull(terminalConnectionFailure(503))
    }
    @Test fun observerConnectionDoesNotAcquireControllerOrAllowSubmission()=runBlocking {
        val server=MockWebServer();val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO);val client=OkHttpClient()
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {}));server.start()
        val transport=WebSocketTransport(client,server.url("/").toString(),"test-credential","session-one",{42},scope,readOnly=true)
        try {
            transport.connect()
            withTimeout(5000) {transport.connectionState().first {it is ConnectionState.Connected}}
            assertEquals("/v1/sessions/session-one/connect?after=42&readOnly=true",server.takeRequest(5,TimeUnit.SECONDS)!!.path)
            try {transport.send("{}".toByteArray());fail("Observer must not submit requests")} catch(error:IllegalStateException) {assertTrue(error.message!!.contains("read-only"))}
        } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
    }
    @Test fun remoteCleartextAndAddressCredentialsAreRejected() {
        listOf("http://192.168.1.2","https://user:secret@host.example","https://host.example/path","https://host.example/?token=secret").forEach { address->
            assertThrows(IllegalArgumentException::class.java) {validateAddress(address,true)}
        }
        assertThrows(IllegalArgumentException::class.java) {validateAddress("http://127.0.0.1",false)}
        assertTrue(validateAddress("https://host.example",false).isHttps)
    }
    @Test fun connectionCarriesCredentialProtocolAndReplayCursor()= runBlocking {
        val server=MockWebServer()
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val client=OkHttpClient()
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) { webSocket.send("{\"type\":\"replay_complete\",\"latestSequence\":42}") }
        }))
        server.start()
        val transport=WebSocketTransport(client,server.url("/").toString(),"test-credential","session-one",{42},scope)
        try {
            val received=async {withTimeout(5000) {transport.messages().first().toString(Charsets.UTF_8)}}
            transport.connect()
            withTimeout(5000) {transport.connectionState().first {it is ConnectionState.Connected}}
            assertTrue(received.await().contains("replay_complete"))
            val request=server.takeRequest(5,TimeUnit.SECONDS)!!
            assertEquals("Bearer test-credential",request.getHeader("Authorization"))
            assertEquals("acpd.v1",request.getHeader("Sec-WebSocket-Protocol"))
            assertEquals("/v1/sessions/session-one/connect?after=42",request.path)
        } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
    }
    @Test fun expiredCredentialsStopReconnectLoop()=runBlocking {
        val server=MockWebServer();server.enqueue(MockResponse().setResponseCode(401));server.start()
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO);val client=OkHttpClient()
        val transport=WebSocketTransport(client,server.url("/").toString(),"expired","s",{0},scope)
        try {
            transport.connect()
            val state=withTimeout(5000) {transport.connectionState().first {it is ConnectionState.Failed}} as ConnectionState.Failed
            assertTrue(state.reason.contains("Pair"));assertEquals(1,server.requestCount)
        } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
    }
    @Test fun reconnectReadsFreshCursorAndDoesNotRepeatPrompt()=runBlocking {
        val server=MockWebServer();val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO);val client=OkHttpClient()
        var cursor=0L
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {cursor=9;webSocket.close(1001,"Reconnect test")}
        }))
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {}));server.start()
        val transport=WebSocketTransport(client,server.url("/").toString(),"credential","s",{cursor},scope)
        try {
            transport.connect()
            val first=withContext(Dispatchers.IO) {server.takeRequest(5,TimeUnit.SECONDS)}!!
            val second=withContext(Dispatchers.IO) {server.takeRequest(5,TimeUnit.SECONDS)}!!
            assertTrue(first.path!!.endsWith("after=0"));assertTrue(second.path!!.endsWith("after=9"))
            assertEquals(0,second.bodySize)
        } finally {transport.disconnect();scope.cancel();server.shutdown();client.dispatcher.executorService.shutdown();client.connectionPool.evictAll()}
    }
}
