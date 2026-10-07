package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

class LocalCopyTest {
    @Test fun removalPreservesCursorPendingPermissionAndHostProcess() = runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val repository=(context.applicationContext as PortalApplication).repository
        val host=repository.hosts.first().firstOrNull()
        assumeTrue("Requires the paired mock host",host!=null)
        val workspace=repository.api.workspaces(host!!).first().path
        val original=repository.workspaceAccess(host.id,workspace)
        repository.saveWorkspaceAccess(host.id,workspace,WorkspaceAccess(false,false,false))
        var stored:dev.acportal.storage.StoredSession?=null
        try {
            stored=repository.create(host.id,"mock",workspace)
            val live=repository.open(host.id,stored.id)
            withTimeout(20_000) {live.connection.first {it is ConnectionState.Connected}}
            if(live.info.acpSessionId.isBlank()) {
                repository.configure(live,"authenticate",buildJsonObject {put("methodId","mock-login")})
                withTimeout(20_000) {live.metadata.first {it.acpSessionId.isNotBlank()}}
            }
            repository.prompt(live,"local-copy-message-marker")
            val waiting=withTimeout(20_000) {live.state.first {it.permissions.isNotEmpty()}}
            repository.draft(host.id,stored.id,"local-copy-draft-marker")
            repository.removeLocalCopy(live)
            assertTrue(live.state.value.items.isEmpty())
            assertEquals(waiting.sequence,live.state.value.sequence)
            assertEquals(waiting.permissions.keys,live.state.value.permissions.keys)
            assertTrue(live.state.value.processing)
            val cached=repository.sessions.first().first {it.id==stored.id}
            assertEquals("",cached.draft)
            assertFalse(cached.state.contains("local-copy-message-marker"))
            assertEquals(waiting.sequence,WireJson.decodeFromString<SessionState>(cached.state).sequence)
            repository.reconnect(live)
            withTimeout(20_000) {live.transport.connectionState().first {it is ConnectionState.Connected}}
            delay(700) // Allow replay and the periodic cache writer to complete.
            assertTrue(live.state.value.items.isEmpty())
            assertEquals(waiting.permissions.keys,live.state.value.permissions.keys)
            assertEquals(live.info.acpSessionId,repository.api.session(host,stored.id).acpSessionId)
            val permission=live.state.value.permissions.values.single()
            val reject=permission.options.first {it.objectValue()["kind"].text()=="reject_once"}.objectValue()["optionId"].text()
            repository.permission(live,permission,reject)
            withTimeout(20_000) {live.state.first {!it.processing && it.permissions.isEmpty()}}
        } finally {
            stored?.let {repository.delete(it)}
            repository.saveWorkspaceAccess(host.id,workspace,original)
        }
        Unit
    }
}
