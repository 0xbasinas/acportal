package dev.acportal.data

import java.net.InetSocketAddress
import java.net.ServerSocket
import java.util.Collections
import java.util.concurrent.Executors

/**
 * Minimal loopback HTTP host for instrumented fixtures. It never launches agents. Each
 * request is answered by [handle] with a status and JSON body; a handler may block to
 * model a slow host. Requests are recorded as "METHOD /path".
 */
class FixtureHost(private val handle:(method:String,path:String,body:String)->Pair<Int,String>):AutoCloseable {
    private val server=ServerSocket().apply {bind(InetSocketAddress("127.0.0.1",0))}
    private val workers=Executors.newCachedThreadPool()
    val requests:MutableList<String> = Collections.synchronizedList(mutableListOf())
    val address get()="http://127.0.0.1:${server.localPort}"
    init {
        Thread {
            try {while(!server.isClosed) {
                val socket=server.accept()
                workers.submit {socket.use {
                    try {
                        socket.soTimeout=30_000
                        val input=socket.getInputStream()
                        val head=StringBuilder()
                        while(!head.endsWith("\r\n\r\n")) {val next=input.read();if(next<0)return@submit;head.append(next.toChar())}
                        val lines=head.lines()
                        val (method,target)=lines.first().split(' ').let {it[0] to it[1]}
                        val length=lines.firstOrNull {it.startsWith("Content-Length:",ignoreCase=true)}?.substringAfter(':')?.trim()?.toIntOrNull() ?: 0
                        val body=ByteArray(length).also {var read=0;while(read<length) {val n=input.read(it,read,length-read);if(n<0)break;read+=n}}.toString(Charsets.UTF_8)
                        val path=target.substringBefore('?')
                        requests.add("$method $path")
                        val (status,reply)=handle(method,path,body)
                        val bytes=reply.toByteArray(Charsets.UTF_8)
                        socket.getOutputStream().apply {
                            write("HTTP/1.1 $status Fixture\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray())
                            write(bytes);flush()
                        }
                    } catch(_:Exception) {}
                }}
            }} catch(_:Exception) {}
        }.apply {isDaemon=true;start()}
    }
    fun count(request:String)=synchronized(requests) {requests.count {it==request}}
    override fun close() {server.close();workers.shutdownNow()}
}
