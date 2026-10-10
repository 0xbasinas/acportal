package dev.acportal.data

import android.net.Uri
import android.os.Bundle
import android.os.Debug
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** ART allocation counters are process-wide, including framework/test allocations. No payload dump. */
class IsolatedAllocationProfileTest {
    @Test fun streamingAndPromptPreparationReportAllocatorCounters() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        assumeTrue("Only the isolated package is profiling evidence", instrumentation.targetContext.packageName == "dev.acportal.acceptance")
        suspend fun measure(name: String, work: suspend () -> Unit) {
            System.gc(); delay(200)
            val allocatedBefore = Debug.getRuntimeStat("art.gc.bytes-allocated").toLong()
            val freedBefore = Debug.getRuntimeStat("art.gc.bytes-freed").toLong()
            work()
            System.gc(); delay(200)
            val allocated = Debug.getRuntimeStat("art.gc.bytes-allocated").toLong() - allocatedBefore
            val freed = Debug.getRuntimeStat("art.gc.bytes-freed").toLong() - freedBefore
            assertTrue(allocated > 0); assertTrue(freed >= 0)
            instrumentation.sendStatus(2, Bundle().apply {
                putString("acportalAllocationProfile", "$name; artAllocatedBytes=$allocated; artFreedBytes=$freed; includes framework/test overhead")
            })
        }
        measure("four streaming histories") {
            AndroidTimelineHeapStressTest().fourLongConversationsRetainBoundedHeapAndVisibleHistoryGaps()
        }
        measure("100 prompt serializations") {
            PromptPreparationStressTest().repeatedLargeAttachmentPreparationAllowsMainQueueProgressAndReleasesCopies()
        }
    }

    @Test fun binaryAttachmentLoadingReportsAllocatorCountersInOwnedPackage() = runBlocking {
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val context = instrumentation.targetContext
        assumeTrue("Use -PacportalAcceptance=true; normal-app runs are not profiling evidence",
            context.packageName == "dev.acportal.acceptance")
        fun allocated() = Debug.getRuntimeStat("art.gc.bytes-allocated").toLong()
        fun freed() = Debug.getRuntimeStat("art.gc.bytes-freed").toLong()
        fun heap() = Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()
        val uri = Uri.parse("content://${context.packageName}.test.attachments/near")
        // Warm up class loading before the measured interval.
        loadAttachment(context, uri, AttachmentKind.FILE)
        System.gc(); delay(200)
        val allocatedBefore = allocated(); val freedBefore = freed(); val heapBefore = heap()
        var encodedCharacters = 0L
        repeat(20) {
            val attachment = loadAttachment(context, uri, AttachmentKind.FILE)
            val resource = attachment.content["resource"] as kotlinx.serialization.json.JsonObject
            val data = resource["blob"] as kotlinx.serialization.json.JsonPrimitive
            assertEquals(349528, data.content.length)
            encodedCharacters += data.content.length
        }
        System.gc(); delay(200)
        val allocationDelta = allocated() - allocatedBefore
        val freedDelta = freed() - freedBefore
        val heapDelta = heap() - heapBefore
        assertTrue("Allocator counters must include the input byte arrays", allocationDelta >= 20L * 262144)
        assertTrue("GC counters must be monotonic", freedDelta >= 0)
        instrumentation.sendStatus(2, Bundle().apply {
            putString("acportalAllocationProfile", "20 binary loads x 262144 bytes; encodedCharacters=$encodedCharacters; artAllocatedBytes=$allocationDelta; artFreedBytes=$freedDelta; sampledPostGcHeapDeltaBytes=$heapDelta")
        })
    }
}
