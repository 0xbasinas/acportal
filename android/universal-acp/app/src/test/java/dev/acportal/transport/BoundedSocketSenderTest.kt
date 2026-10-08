package dev.acportal.transport

import okhttp3.Request
import okhttp3.WebSocket
import okio.ByteString
import org.junit.Assert.*
import org.junit.Test
import java.util.concurrent.Executors

class BoundedSocketSenderTest {
    private class Socket : WebSocket {
        var queued = 0L
        var sends = 0
        var accepts = true
        override fun request() = Request.Builder().url("http://localhost/").build()
        override fun queueSize() = queued
        override fun send(text: String): Boolean {
            if (!accepts) return false
            sends++
            queued += text.toByteArray(Charsets.UTF_8).size
            return true
        }
        override fun send(bytes: ByteString) = error("Text frames required")
        override fun close(code: Int, reason: String?) = true
        override fun cancel() = Unit
    }

    @Test fun fullQueueRejectsBeforeSendAndDrainAllowsExplicitRetry() {
        val socket = Socket()
        val sender = BoundedSocketSender(10)
        sender.send(socket, ByteArray(10) { 'x'.code.toByte() })
        assertThrows(IllegalStateException::class.java) { sender.send(socket, byteArrayOf(120)) }
        assertEquals(1, socket.sends)
        socket.queued = 0
        sender.send(socket, byteArrayOf(120))
        assertEquals(2, socket.sends)
        socket.accepts = false
        assertThrows(IllegalStateException::class.java) { sender.send(socket, byteArrayOf(120)) }
    }

    @Test fun decodedUtf8ExpansionCannotBypassFrameOrQueueLimits() {
        val socket = Socket()
        assertThrows(IllegalStateException::class.java) { BoundedSocketSender(2).send(socket, byteArrayOf(-1)) }
        assertThrows(IllegalArgumentException::class.java) {
            BoundedSocketSender().send(socket, ByteArray(1024 * 1024) { -1 })
        }
        assertEquals(0, socket.sends)
    }

    @Test fun concurrentSubmissionsCannotRacePastByteBudget() {
        val socket = Socket()
        val sender = BoundedSocketSender(1024)
        val workers = Executors.newFixedThreadPool(8)
        try {
            val tasks = (0 until 8).map { workers.submit<Boolean> {
                try { sender.send(socket, ByteArray(256) { 120 }); true }
                catch (_: IllegalStateException) { false }
            } }
            assertEquals(4, tasks.count { it.get() })
            assertEquals(1024L, socket.queued)
            assertEquals(4, socket.sends)
        } finally { workers.shutdownNow() }
    }
}
