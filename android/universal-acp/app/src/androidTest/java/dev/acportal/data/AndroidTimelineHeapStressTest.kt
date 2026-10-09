package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

/** Fixture text only. No heap dump, network, database or production payload capture. */
class AndroidTimelineHeapStressTest {
    @Test fun fourLongConversationsRetainBoundedHeapAndVisibleHistoryGaps()=runBlocking {
        fun used()=Runtime.getRuntime().totalMemory()-Runtime.getRuntime().freeMemory()
        val states=Array(4) {SessionState()}
        System.gc();delay(100)
        val baseline=used();var peak=baseline
        withContext(Dispatchers.Default) {
            repeat(600) {index->repeat(4) {session->
                val envelope=buildJsonObject {
                    put("type","event");put("sequence",index+1);put("direction","agent")
                    putJsonObject("message") {put("method","session/update");putJsonObject("params") {
                        putJsonObject("update") {
                            put("sessionUpdate",if(index%2==0)"user_message_chunk" else "agent_message_chunk")
                            putJsonObject("content") {put("type","text");put("text","Session $session event $index "+"x".repeat(64*1024))}
                        }
                    }}
                }
                states[session]=SessionReducer.reduce(states[session],envelope)
                peak=maxOf(peak,used())
            }}
        }
        System.gc();delay(100)
        val retained=used()-baseline
        states.forEachIndexed {session,state->
            assertTrue(state.historyGap);assertEquals(600L,state.sequence)
            assertTrue(state.items.size<600)
            assertTrue((state.items.last() as TimelineItem.Text).text.startsWith("Session $session event 599"))
        }
        InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {
            putString("acportalTimelineHeap","4 sessions x 600 x 64KiB; retainedBytes=$retained; sampledPeakGrowthBytes=${peak-baseline}; items=${states.map {it.items.size}}")
        })
        assertTrue("Conversation histories retained unbounded heap",retained<48L*1024*1024)
        assertTrue("Sampled timeline allocation peak exceeded budget",peak-baseline<96L*1024*1024)
    }
}
