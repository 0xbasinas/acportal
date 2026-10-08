package dev.acportal.transport

import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class IncomingMessageQueueTest {
    @Test fun bytePressurePreservesOrderAndReleasesCapacityOnConsumption() = runBlocking {
        val queue = IncomingMessageQueue(10)
        val first = byteArrayOf(1, 2, 3, 4, 5, 6)
        val second = byteArrayOf(7, 8, 9, 10)
        assertTrue(queue.offer(first))
        assertTrue(queue.offer(second))
        assertFalse(queue.offer(byteArrayOf(11)))
        assertArrayEquals(first, queue.messages().first())
        assertTrue(queue.offer(byteArrayOf(11)))
        assertArrayEquals(second, queue.messages().first())
        assertArrayEquals(byteArrayOf(11), queue.messages().first())
        assertTrue(queue.offer(ByteArray(10)))
        assertFalse(queue.offer(ByteArray(11)))
    }

    @Test fun countPressureDoesNotLeakByteReservation() = runBlocking {
        val queue = IncomingMessageQueue(1000)
        repeat(256) { assertTrue(queue.offer(byteArrayOf(1))) }
        assertFalse(queue.offer(ByteArray(500)))
        repeat(256) { assertEquals(1, queue.messages().first().size) }
        assertTrue(queue.offer(ByteArray(1000)))
    }
}
