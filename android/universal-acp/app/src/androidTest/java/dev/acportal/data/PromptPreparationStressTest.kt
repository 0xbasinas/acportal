package dev.acportal.data

import android.os.Debug
import android.os.SystemClock
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import dev.acportal.transport.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

/** No storage writes or network. Measures bounded submissions from the real Main dispatcher. */
class PromptPreparationStressTest {
    private val repository get()=(InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication).repository
    private class Sink:AgentTransport {
        var sends=0
        override suspend fun connect()=Unit
        override suspend fun disconnect()=Unit
        override suspend fun send(message:ByteArray) {
            assertTrue(message.size<=MAX_PROMPT_WIRE_BYTES)
            val wire=WireJson.parseToJsonElement(message.toString(Charsets.UTF_8)).objectValue()
            assertEquals("session/prompt",wire["method"].text())
            assertEquals("fixture-acp",wire["params"].objectValue()["sessionId"].text())
            sends++
        }
        override fun messages()=emptyFlow<ByteArray>()
        override fun connectionState()=flowOf(ConnectionState.Connected)
    }
    private fun live(sink:Sink)=LiveSession(HostProfile("fixture","Fixture","https://example.invalid","unused","fixture",0),
        SessionInfo("fixture-session","fixture-agent","fixture-acp","/fixture",initialization=buildJsonObject {
            putJsonObject("agentCapabilities") {putJsonObject("promptCapabilities") {put("embeddedContext",true)}}
        }),sink,SessionState()).also {it.connection.value=ConnectionState.Connected}
    private fun attachment()=PromptAttachment("fixture.txt",buildJsonObject {
        put("type","resource");putJsonObject("resource") {put("uri","file:///fixture.txt");put("mimeType","text/plain");put("text","x".repeat(256*1024))}
    })
    @Test fun preparationCannotSendAfterReplayStopDetachOrLocalClear()=runBlocking {
        withContext(Dispatchers.Main) {
            for(change in 0..4) {
                val sink=Sink();val live=live(sink)
                val selected=listOf(attachment());live.attachments.value=selected
                var refused=false
                val job=launch(start=CoroutineStart.UNDISPATCHED) {
                    try {repository.prompt(live,"Fixture prompt");fail("Prepared stale mutation sent")}
                    catch(_:IllegalStateException) {refused=true}
                }
                when(change) {
                    0->live.state.value=live.state.value.copy(replaying=true)
                    1->live.state.value=live.state.value.copy(sessionClosed=true)
                    2->live.manuallyDetached=true
                    3->live.localCopyGeneration.value++
                    4->live.state.value=live.state.value.copy(processing=true)
                }
                job.join()
                assertTrue(refused);assertEquals(0,sink.sends)
                assertEquals(selected,live.attachments.value);assertNull(live.submittedAttachments)
            }
        }
    }
    @Test fun repeatedLargeAttachmentPreparationAllowsMainQueueProgressAndReleasesCopies()=runBlocking {
        val sink=Sink();val live=live(sink);val selected=listOf(attachment())
        fun used()=Runtime.getRuntime().totalMemory()-Runtime.getRuntime().freeMemory()
        System.gc();delay(100)
        val baseline=used();var peak=baseline;var ticks=0;var longest=0L
        withContext(Dispatchers.Main) {
            val heartbeat=launch {var previous=SystemClock.uptimeMillis();while(isActive) {
                delay(10);val now=SystemClock.uptimeMillis();longest=maxOf(longest,now-previous);previous=now;ticks++
            }}
            try {repeat(100) {
                repository.prompt(live,"Fixture attachment prompt",attachments=selected)
                live.state.value=SessionState();live.submittedAttachments=null
                peak=maxOf(peak,used())
            }} finally {heartbeat.cancelAndJoin()}
        }
        assertEquals(100,sink.sends);assertTrue("Main queue did not progress",ticks>0)
        System.gc();delay(100)
        val retained=used()-baseline
        val memory=Debug.MemoryInfo().also {Debug.getMemoryInfo(it)}
        InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {
            putString("acportalPromptMemory","100 submissions; retainedBytes=$retained; sampledPeakGrowthBytes=${peak-baseline}; mainTicks=$ticks; longestTickMs=$longest; totalPssKiB=${memory.totalPss}")
        })
        assertTrue("Repeated prompt copies retained",retained<16L*1024*1024)
        assertTrue("Allocation peak exceeded scoped regression budget",peak-baseline<64L*1024*1024)
    }
    @Test fun providerBoundaryFilesLoadAndSerializeWithoutBlockingMainQueue()=runBlocking {
        val sink=Sink();val live=live(sink)
        fun used()=Runtime.getRuntime().totalMemory()-Runtime.getRuntime().freeMemory()
        System.gc();delay(100)
        val baseline=used();var peak=baseline;var ticks=0
        withContext(Dispatchers.Main) {
            val heartbeat=launch {while(isActive) {delay(10);ticks++}}
            try {repeat(30) {
                val loaded=loadAttachment(InstrumentationRegistry.getInstrumentation().targetContext,
                    android.net.Uri.parse("content://dev.acportal.test.attachments/near"),AttachmentKind.FILE)
                assertEquals(256*1024,android.util.Base64.decode(loaded.content["resource"].objectValue()["blob"].text(),android.util.Base64.DEFAULT).size)
                assertFalse(loaded.content.toString().contains("content://"))
                repository.prompt(live,"Fixture provider prompt",attachments=listOf(loaded))
                live.state.value=SessionState();live.submittedAttachments=null
                peak=maxOf(peak,used())
            }} finally {heartbeat.cancelAndJoin()}
        }
        System.gc();delay(100)
        val retained=used()-baseline
        assertEquals(30,sink.sends);assertTrue("Provider path blocked the Main queue",ticks>0)
        InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {
            putString("acportalProviderMemory","30 x 256KiB provider files; retainedBytes=$retained; sampledPeakGrowthBytes=${peak-baseline}; mainTicks=$ticks")
        })
        assertTrue(retained<16L*1024*1024);assertTrue(peak-baseline<64L*1024*1024)
    }
}
