package dev.acportal.transport

import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/** Byte ownership ends when a consumer receives a frame, before downstream parsing. */
internal class IncomingMessageQueue(private val byteLimit: Int = 8 * 1024 * 1024) {
    private val lock = Any()
    private var bytes = 0
    private val channel = Channel<ByteArray>(256, onUndeliveredElement = ::release)

    init { require(byteLimit > 0) }

    fun offer(message: ByteArray): Boolean = synchronized(lock) {
        if (message.size > byteLimit - bytes) return@synchronized false
        bytes += message.size
        if (channel.trySend(message).isSuccess) true else {
            bytes -= message.size
            false
        }
    }

    private fun release(message: ByteArray) = synchronized(lock) { bytes -= message.size }

    fun messages(): Flow<ByteArray> = flow {
        for (message in channel) {
            release(message)
            emit(message)
        }
    }
}
