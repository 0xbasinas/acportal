package dev.acportal.data

import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import okhttp3.*
import okio.ByteString
import okio.ByteString.Companion.toByteString
import java.nio.ByteBuffer
import kotlin.random.Random

/** What a terminal screen needs from one attachment: keystrokes and size out, output and events in. */
interface TerminalLink {
    fun input(text:String)
    fun resize(rows:Int,cols:Int)
    fun takeControl()
    fun close()
}

/**
 * One attachment to a host terminal (`/v1/terminals/{id}/ws`). Output arrives as binary frames
 * (8-byte big-endian byte offset, then bytes); after a drop the socket resumes from the last byte it
 * delivered, so nothing is shown twice. The token stays in this class and never reaches the web page.
 */
class TerminalSocket(
    private val client:OkHttpClient,private val address:String,private val token:String,private val terminalId:String,
    private val scope:CoroutineScope,
    private val onData:(ByteArray)->Unit,
    private val onEvent:(JsonObject)->Unit,
    private val onState:(state:String,reason:String?)->Unit,
) : TerminalLink {
    @Volatile private var socket:WebSocket? = null
    @Volatile private var cursor = 0L
    @Volatile private var stopped = false
    @Volatile private var rows = 0
    @Volatile private var cols = 0
    private var job:Job? = null

    fun start() {
        if (job?.isActive == true) return
        job = scope.launch(Dispatchers.IO) {
            var attempt = 0
            while (isActive && !stopped) {
                onState(if (attempt == 0) "connecting" else "reconnecting",null)
                val finished = CompletableDeferred<Int?>()
                var opened = false
                val endpoint = validateAddress(address).newBuilder().encodedPath("/v1/terminals/$terminalId/ws").addQueryParameter("after",cursor.toString()).build().toString().replaceFirst("https:","wss:").replaceFirst("http:","ws:")
                val request = Request.Builder().url(endpoint).header("Authorization","Bearer $token").header("Sec-WebSocket-Protocol","acpd.terminal.v1").build()
                val current = client.newWebSocket(request,object:WebSocketListener() {
                    override fun onOpen(webSocket:WebSocket,response:Response) { opened=true;attempt=0;socket=webSocket;onState("connected",null);if(rows>0)webSocket.send(JsonObject(mapOf("type" to JsonPrimitive("resize"),"rows" to JsonPrimitive(rows),"cols" to JsonPrimitive(cols))).toString()) }
                    override fun onMessage(webSocket:WebSocket,text:String) { try {onEvent(Json.parseToJsonElement(text).jsonObject)} catch(_:Exception) {} }
                    override fun onMessage(webSocket:WebSocket,bytes:ByteString) {
                        if (bytes.size < 8) return
                        val offset = ByteBuffer.wrap(bytes.substring(0,8).toByteArray()).long
                        val body = bytes.substring(8).toByteArray()
                        if (offset + body.size <= cursor) return
                        val skip = maxOf(0L,cursor - offset).toInt()
                        cursor = offset + body.size
                        onData(if (skip == 0) body else body.copyOfRange(skip,body.size))
                    }
                    override fun onClosing(webSocket:WebSocket,code:Int,reason:String) { webSocket.close(code,reason) }
                    override fun onClosed(webSocket:WebSocket,code:Int,reason:String) { finished.complete(null) }
                    override fun onFailure(webSocket:WebSocket,t:Throwable,response:Response?) { finished.complete(response?.code) }
                })
                socket = current
                val code = try { finished.await() } finally { current.cancel();socket = null }
                if (stopped) break
                when (code) {
                    401 -> { onState("failed","This phone is no longer paired with the host");return@launch }
                    403 -> { onState("failed","The host does not allow terminals for this phone");return@launch }
                    404 -> { onState("failed","This terminal has ended on the host");return@launch }
                }
                if (!opened) attempt++
                if (attempt > 8) { onState("failed","The connection to the host was lost");return@launch }
                onState("reconnecting",null)
                val wait = minOf(10_000L,500L shl minOf(attempt,5))
                delay(wait/2 + Random.nextLong(wait/2 + 1))
            }
        }
    }

    override fun input(text:String) { socket?.send(text.toByteArray().toByteString()) }
    override fun resize(rows:Int,cols:Int) {
        this.rows = rows;this.cols = cols
        socket?.send(JsonObject(mapOf("type" to JsonPrimitive("resize"),"rows" to JsonPrimitive(rows),"cols" to JsonPrimitive(cols))).toString())
    }
    override fun takeControl() { socket?.send("""{"type":"take"}""") }
    override fun close() { stopped = true;job?.cancel();socket?.close(1000,"Client detached");socket = null;onState("closed",null) }
}
