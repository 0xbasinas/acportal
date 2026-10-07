package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

class LiveSessionActivityHostTest {
    @Test fun concurrentSessionsKeepIndependentActivityThroughReconnectAndDeletion()=runBlocking {
        val repository=(InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication).repository
        val host=repository.hosts.first().firstOrNull()
        assumeTrue("Requires the paired mock host",host!=null)
        val workspace=repository.api.workspaces(host!!).first().path
        val created=mutableListOf<StoredSession>()
        suspend fun activity(id:String,expected:LiveSessionActivity) {
            withTimeout(20_000) {repository.liveActivities.first {it["${host.id}/$id"]==expected}}
        }
        suspend fun open():LiveSession {
            val stored=repository.create(host.id,"mock",workspace);created+=stored
            val live=repository.open(host.id,stored.id)
            activity(stored.id,LiveSessionActivity.READY)
            if(live.info.acpSessionId.isBlank()) {
                repository.configure(live,"authenticate",buildJsonObject {put("methodId","mock-login")})
                withTimeout(20_000) {live.metadata.first {it.acpSessionId.isNotBlank()}}
            }
            return live
        }
        suspend fun reject(live:LiveSession) {
            val permission=live.state.value.permissions.values.single()
            val option=permission.options.first {it.objectValue()["kind"].text()=="reject_once"}.objectValue()["optionId"].text()
            repository.permission(live,permission,option)
            activity(live.info.id,LiveSessionActivity.READY)
        }
        try {
            val one=open();val two=open()
            repository.prompt(one,"Review the first workspace")
            activity(one.info.id,LiveSessionActivity.WAITING_APPROVAL)
            assertEquals(LiveSessionActivity.READY,repository.liveActivities.value["${host.id}/${two.info.id}"])
            repository.prompt(two,"Review the second workspace")
            activity(two.info.id,LiveSessionActivity.WAITING_APPROVAL)
            reject(one)
            assertEquals(LiveSessionActivity.WAITING_APPROVAL,repository.liveActivities.value["${host.id}/${two.info.id}"])
            val pending=two.state.value.permissions.values.single()
            repository.disconnect(two)
            activity(two.info.id,LiveSessionActivity.DISCONNECTED)
            val obsolete=Permission(JsonPrimitive("obsolete-fixture"),pending.request)
            two.state.value=two.state.value.copy(permissions=two.state.value.permissions+(obsolete.id.toString() to obsolete))
            repository.reconnect(two)
            activity(two.info.id,LiveSessionActivity.WAITING_APPROVAL)
            assertEquals(listOf(pending),two.state.value.permissions.values.toList())
            reject(two)
            repository.disconnect(two)
            activity(two.info.id,LiveSessionActivity.DISCONNECTED)
            two.state.value=two.state.value.copy(permissions=mapOf(pending.id.toString() to pending))
            repository.reconnect(two)
            activity(two.info.id,LiveSessionActivity.READY)
            assertTrue(two.state.value.permissions.isEmpty())
            val first=created.first();repository.delete(first);created.remove(first)
            assertFalse(repository.liveActivities.value.containsKey("${host.id}/${one.info.id}"))
            assertEquals(LiveSessionActivity.READY,repository.liveActivities.value["${host.id}/${two.info.id}"])
        } finally {created.toList().forEach {repository.delete(it)}}
        Unit
    }
}
