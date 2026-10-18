package dev.acportal.data

import kotlinx.coroutines.*
import kotlinx.serialization.json.JsonObject
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.Assert.*
import org.junit.Test
import java.nio.ByteBuffer
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.TimeUnit

class TerminalSocketTest {
    private fun frame(offset:Long,text:String):ByteString =
        (ByteBuffer.allocate(8).putLong(offset).array() + text.toByteArray()).toByteString()

    @Test fun reconnectsFromTheLastByteAndNeverShowsOutputTwice() = runBlocking {
        val server = MockWebServer()
        // First attachment: two pieces, then the connection dies. Second: overlaps the first, then adds new output.
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {
                webSocket.send(frame(0,"abc"));webSocket.send(frame(3,"def"))
                webSocket.close(1011,"drop")
            }
        }))
        server.enqueue(MockResponse().withWebSocketUpgrade(object:WebSocketListener() {
            override fun onOpen(webSocket:WebSocket,response:Response) {
                webSocket.send(frame(4,"efg"));webSocket.send("""{"type":"exit","code":3}""")
            }
        }))
        server.start()
        val output = StringBuilder()
        val events = CopyOnWriteArrayList<JsonObject>()
        val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
        val socket = TerminalSocket(OkHttpClient(),server.url("/").toString(),"token","t1",scope,{synchronized(output) {output.append(String(it))}},{events.add(it)},{_,_ ->})
        socket.start()
        withTimeout(10_000) { while (events.isEmpty()) delay(20) }
        assertEquals("abcdefg",synchronized(output) {output.toString()})
        val first = server.takeRequest(5,TimeUnit.SECONDS)!!
        val second = server.takeRequest(5,TimeUnit.SECONDS)!!
        assertEquals("/v1/terminals/t1/ws?after=0",first.path)
        assertEquals("/v1/terminals/t1/ws?after=6",second.path)
        assertEquals("Bearer token",second.getHeader("Authorization"))
        assertEquals("acpd.terminal.v1",second.getHeader("Sec-WebSocket-Protocol"))
        assertEquals("exit",events.first()["type"].toString().trim('"'))
        socket.close();scope.cancel();try {server.shutdown()} catch(_:java.io.IOException) {}
    }

    @Test fun theKeysRowOffersWhatAPhoneKeyboardLacks() {
        val keys = TERMINAL_KEYS_FOR_TEST
        assertEquals("\u001b",keys["Esc"]);assertEquals("\t",keys["Tab"]);assertEquals("\u001b[A",keys["↑"])
    }
    private val TERMINAL_KEYS_FOR_TEST get() = dev.acportal.presentation.TERMINAL_KEYS.toMap()
}
