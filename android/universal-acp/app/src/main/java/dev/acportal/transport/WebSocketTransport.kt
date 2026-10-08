package dev.acportal.transport

import dev.acportal.data.validateAddress
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import okhttp3.*
import okio.ByteString
import kotlin.random.Random

class WebSocketTransport(
    private val client:OkHttpClient,private val address:String,private val token:String,
    private val sessionId:String,private val cursor:()->Long,private val scope:CoroutineScope,
    private val readOnly:Boolean=false,
) : AgentTransport {
    private val incoming = IncomingMessageQueue()
    private val sender = BoundedSocketSender()
    private val state = MutableStateFlow<ConnectionState>(ConnectionState.Disconnected)
    private var job:Job? = null
    @Volatile private var socket:WebSocket? = null
    override fun messages():Flow<ByteArray> = incoming.messages()
    override fun connectionState():StateFlow<ConnectionState> = state.asStateFlow()
    override suspend fun connect() {
        if (job?.isActive == true) return
        job = scope.launch {
            var attempt = 0
            try {
                while (isActive) {
                    state.value = if (attempt == 0) ConnectionState.Connecting else ConnectionState.Reconnecting(attempt)
                    val completed = CompletableDeferred<Int?>()
                    val opened = CompletableDeferred<Unit>()
                    val base = validateAddress(address)
                    val endpoint = base.newBuilder().encodedPath("/v1/sessions/$sessionId/connect").addQueryParameter("after",cursor().toString()).apply {if(readOnly)addQueryParameter("readOnly","true")}.build().toString().replaceFirst("https:","wss:").replaceFirst("http:","ws:")
                    val request = Request.Builder().url(endpoint).header("Authorization","Bearer $token").header("Sec-WebSocket-Protocol","acpd.v1").build()
                    val current = client.newWebSocket(request,object:WebSocketListener() {
                        override fun onOpen(webSocket:WebSocket,response:Response) { socket=webSocket; state.value=ConnectionState.Connected;opened.complete(Unit) }
                        override fun onMessage(webSocket:WebSocket,text:String) {
                            val bytes = text.toByteArray()
                            if (bytes.size > 1024*1024+4096 || !incoming.offer(bytes)) { webSocket.cancel();completed.complete(null) }
                        }
                        override fun onMessage(webSocket:WebSocket,bytes:ByteString) { webSocket.close(1003,"Use JSON text frames");completed.complete(null) }
                        override fun onClosed(webSocket:WebSocket,code:Int,reason:String) { completed.complete(null) }
                        override fun onFailure(webSocket:WebSocket,t:Throwable,response:Response?) { completed.complete(response?.code) }
                        override fun onClosing(webSocket:WebSocket,code:Int,reason:String) { webSocket.close(code,reason) }
                    })
                    socket=current
                    val code = try { withTimeout(20_000) { selectOpenOrClose(opened,completed) };completed.await() }
                        catch (_:TimeoutCancellationException) { null }
                        finally { current.cancel();socket=null }
                    terminalConnectionFailure(code)?.let {state.value=it;return@launch}
                    if(opened.isCompleted)attempt=0
                    attempt++
                    state.value=ConnectionState.Reconnecting(attempt)
                    val maximum = minOf(60_000L,1000L shl minOf(attempt-1,6))
                    delay(maximum/2+Random.nextLong(maximum/2+1))
                }
            } finally { socket?.cancel();socket=null }
        }
    }
    private suspend fun selectOpenOrClose(opened:Deferred<Unit>,closed:Deferred<Int?>) {
        kotlinx.coroutines.selects.select<Unit> { opened.onAwait { };closed.onAwait { } }
    }
    override suspend fun disconnect() { job?.cancelAndJoin();job=null;socket?.close(1000,"Client detached");socket=null;state.value=ConnectionState.Disconnected }
    override suspend fun send(message:ByteArray) {
        check(!readOnly) {"This connection is read-only"}
        val current = socket
        check(state.value == ConnectionState.Connected && current != null) { "Wait for the connection to recover before sending" }
        sender.send(current,message)
    }
}
