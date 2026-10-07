package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

/** Run after force-stopping the app. No Activity/ViewModel or host refresh is started here. */
class AgentCatalogStartupTest {
    @Test fun savedCatalogIsAvailableAtFreshApplicationStartupWithoutDiscovery()=runBlocking {
        val application=InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication
        val repository=application.repository
        val hosts=repository.hosts.first()
        val catalog=repository.agentCatalogs.first().firstOrNull {row->hosts.any {it.id==row.hostId} && WireJson.decodeFromString<List<AgentInfo>>(row.metadata).any {it.id=="mock"}}
        assertNotNull("Requires the previously discovered paired mock host",catalog)
        assertTrue(catalog!!.discoveredAt>0)
        val agents=WireJson.decodeFromString<List<AgentInfo>>(catalog.metadata)
        val mock=agents.single {it.id=="mock"}
        assertEquals("Mock ACP",mock.name)
        assertTrue(mock.installed)
        assertFalse(mock.executable.isNullOrBlank())
        Unit
    }
}
