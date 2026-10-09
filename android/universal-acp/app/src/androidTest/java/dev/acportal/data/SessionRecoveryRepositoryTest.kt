package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import dev.acportal.transport.*
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import kotlinx.serialization.json.*

class SessionRecoveryRepositoryTest {
    private val repository get()=(InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication).repository
    private class CountingTransport:AgentTransport {
        var operations=0
        override suspend fun connect() {operations++}
        override suspend fun disconnect() {operations++}
        override suspend fun send(message:ByteArray) {operations++}
        override fun messages()=emptyFlow<ByteArray>()
        override fun connectionState()=MutableStateFlow<ConnectionState>(ConnectionState.Connected)
    }
    private fun live(transport:AgentTransport,state:SessionState)=LiveSession(HostProfile("fixture","Fixture","https://host.example","unused","fixture",0),SessionInfo("s","mock","acp","/workspace"),transport,state)
    @Test fun dismissChangesOnlyTheErrorAndDoesNotSendAnything() {
        val transport=CountingTransport()
        val before=SessionState(sequence=7,error="Agent declined",processing=true,promptRequestId="pending",items=listOf(TimelineItem.Text("t","user","Retained message")),permissions=mapOf("p" to Permission(kotlinx.serialization.json.JsonPrimitive("p"),kotlinx.serialization.json.JsonObject(emptyMap()))))
        val session=live(transport,before)
        session.attachments.value=listOf(contextReference("file:///workspace/readme.md"))
        repository.dismissSessionError(session)
        assertEquals(before.copy(error=null),session.state.value)
        assertEquals(1,session.attachments.value.size);assertEquals(0,transport.operations)
    }
    @Test fun stoppedSessionCannotReconnectOrSubmitAfterDismissal()=runBlocking {
        val transport=CountingTransport()
        val session=live(transport,SessionState(error="Agent exited",errorOrigin=SessionErrorOrigin.SESSION,sessionClosed=true))
        repository.dismissSessionError(session)
        try {repository.reconnect(session);fail("Stopped process must not reconnect")} catch(_:IllegalStateException) {}
        try {repository.prompt(session,"Do not submit");fail("Stopped process must not receive a prompt")} catch(_:IllegalStateException) {}
        assertTrue(session.state.value.sessionClosed);assertEquals(0,transport.operations)
        Unit
    }
    @Test fun onlyCurrentPermissionOnFreshConnectedSessionCanBeSent()=runBlocking {
        val transport=CountingTransport()
        val permission=Permission(JsonPrimitive("request"),buildJsonObject {putJsonArray("options") {add(buildJsonObject {put("optionId","allow");put("kind","allow_once")})}})
        val ready=SessionState(permissions=mapOf(permission.id.toString() to permission))
        val session=live(transport,ready)
        suspend fun denied() {try {repository.permission(session,permission,"allow");fail("Must not send stale permission") } catch(_:IllegalStateException) {};assertEquals(0,transport.operations)}
        denied()
        session.connection.value=ConnectionState.Connected
        session.state.value=ready.copy(replaying=true);denied()
        session.state.value=ready.copy(sessionClosed=true);denied()
        session.state.value=ready.copy(permissions=emptyMap());denied()
        session.state.value=ready.copy(permissions=mapOf(permission.id.toString() to permission.copy(request=JsonObject(emptyMap()))));denied()
        session.state.value=ready;session.manuallyDetached=true;denied()
        session.manuallyDetached=false
        repository.permission(session,permission,"allow")
        assertEquals(1,transport.operations);assertEquals(ready,session.state.value)
        Unit
    }
    @Test fun restoredBusyTurnCannotCancelUntilFreshAndConnected()=runBlocking {
        val transport=CountingTransport()
        val session=live(transport,SessionState(processing=true))
        suspend fun refused() {try {repository.cancel(session);fail("Stale turn cancellation sent")} catch(_:IllegalStateException) {};assertEquals(0,transport.operations)}
        refused();session.connection.value=ConnectionState.Connected
        session.state.value=SessionState(processing=true,replaying=true);refused()
        session.state.value=SessionState(processing=true,sessionClosed=true);refused()
        session.state.value=SessionState();refused()
        session.state.value=SessionState(processing=true);session.manuallyDetached=true;refused()
        session.manuallyDetached=false;repository.cancel(session)
        assertEquals(1,transport.operations)
        Unit
    }
}
