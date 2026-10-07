package dev.acportal.data

import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.transport.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

class AttachmentHostTest {
    @Test fun selectedFilesRemainLocalUntilSendThenReachTheAgent()=runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val repository=(context.applicationContext as PortalApplication).repository
        val host=repository.hosts.first().firstOrNull()
        assumeTrue("Requires the paired media-capable mock host",host!=null)
        val workspace=repository.api.workspaces(host!!).first().path
        val original=repository.workspaceAccess(host.id,workspace)
        repository.saveWorkspaceAccess(host.id,workspace,WorkspaceAccess(false,false,false))
        var stored:dev.acportal.storage.StoredSession?=null
        var observer:WebSocketTransport?=null
        val observation=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        try {
            stored=repository.create(host.id,"mock",workspace)
            val live=repository.open(host.id,stored.id)
            withTimeout(20_000) {live.connection.first {it is ConnectionState.Connected}}
            assertTrue(live.info.promptCapability("embeddedContext"))
            assertTrue(live.info.promptCapability("image"))
            assertTrue(live.info.promptCapability("audio"))
            if(live.info.acpSessionId.isBlank()) {
                repository.configure(live,"authenticate",buildJsonObject {put("methodId","mock-login")})
                withTimeout(20_000) {live.metadata.first {it.acpSessionId.isNotBlank()}}
            }
            observer=WebSocketTransport(repository.api.client,host.address,repository.api.token(host),stored.id,{0},observation,readOnly=true)
            observer.connect()
            withTimeout(20_000) {observer.connectionState().first {it is ConnectionState.Connected}}
            suspend fun readPrompt(id:String):JsonObject=coroutineScope {
                val reply=async(start=CoroutineStart.UNDISPATCHED) {withTimeout(20_000) {observer.messages().map {WireJson.parseToJsonElement(it.toString(Charsets.UTF_8)).objectValue()["message"].objectValue()}.first {it["id"].text()==id && it["result"]!=null}["result"].objectValue()}}
                live.transport.send(request(id,"_mock/last_prompt",buildJsonObject {}).toString().toByteArray())
                reply.await()
            }
            val attachments=listOf("text" to AttachmentKind.FILE,"image" to AttachmentKind.IMAGE,"audio" to AttachmentKind.AUDIO).map {(name,kind)->loadAttachment(context,Uri.parse("content://dev.acportal.test.attachments/$name"),kind)}
            attachments.forEach {repository.addAttachment(live,it)}
            assertTrue(readPrompt("before-send").isEmpty())
            assertEquals(3,live.attachments.value.size)
            repository.prompt(live,"Review selected phone files")
            withTimeout(20_000) {live.state.first {it.permissions.isNotEmpty()}}
            val received=readPrompt("after-send")["prompt"].arrayValue()
            assertEquals(promptContent("Review selected phone files",attachments),received)
            assertTrue(live.attachments.value.isEmpty())
            assertFalse(WireJson.encodeToString(SessionState.serializer(),live.state.value).contains("iVBOR"))
            val permission=live.state.value.permissions.values.single()
            val reject=permission.options.first {it.objectValue()["kind"].text()=="reject_once"}.objectValue()["optionId"].text()
            repository.permission(live,permission,reject)
            withTimeout(20_000) {live.state.first {!it.processing && it.permissions.isEmpty()}}
        } finally {
            observer?.disconnect();observation.cancel()
            stored?.let {repository.delete(it)}
            repository.saveWorkspaceAccess(host.id,workspace,original)
        }
        Unit
    }
}
