package dev.acportal.transport

import okhttp3.WebSocket

internal class BoundedSocketSender(private val byteLimit: Long = 4L * 1024 * 1024) {
    private val lock = Any()

    init { require(byteLimit > 0) }

    fun send(socket: WebSocket, message: ByteArray) = synchronized(lock) {
        require(message.size <= 1024 * 1024) { "Message is too large" }
        val text = message.toString(Charsets.UTF_8)
        val encodedSize = text.toByteArray(Charsets.UTF_8).size
        require(encodedSize <= 1024 * 1024) { "Message is too large" }
        check(socket.queueSize() <= byteLimit - encodedSize) {
            "Outgoing queue is full. Wait before trying again."
        }
        check(socket.send(text)) { "Wait for the connection to recover before sending" }
    }
}
