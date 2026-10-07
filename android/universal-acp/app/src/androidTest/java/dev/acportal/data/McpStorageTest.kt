package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.WireJson
import dev.acportal.security.CredentialVault
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.util.UUID

class McpStorageTest {
    @Test fun encryptedDefinitionsReopenAndRotateAliases() = runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val repository=(context.applicationContext as PortalApplication).repository
        val host=repository.hosts.first().firstOrNull()
        assumeTrue("Requires a paired test connection",host!=null)
        val original=repository.mcpServers(host!!.id)
        val definition=McpDefinition(UUID.randomUUID().toString(),"Storage fixture","http","https://example.com/mcp?key=synthetic-url-secret",values=listOf(McpValue("Authorization","synthetic-header-secret")))
        try {
            repository.saveMcpServers(host.id,listOf(definition))
            val alias=repository.settings.mcpAliases.first()[host.id]!!
            val reopened=WireJson.decodeFromString<List<McpDefinition>>(CredentialVault(context).read(alias))
            assertEquals(listOf(definition),reopened)
            val ciphertext=File(context.noBackupFilesDir,"credentials/$alias").readBytes().toString(Charsets.ISO_8859_1)
            val preferences=File(context.filesDir,"datastore/settings.preferences_pb").readBytes().toString(Charsets.ISO_8859_1)
            listOf("synthetic-url-secret","synthetic-header-secret").forEach {secret->assertFalse(ciphertext.contains(secret));assertFalse(preferences.contains(secret))}
            repository.saveMcpServers(host.id,listOf(definition.copy(enabled=false)))
            assertNotEquals(alias,repository.settings.mcpAliases.first()[host.id])
            assertFalse(File(context.noBackupFilesDir,"credentials/$alias").exists())
            assertFalse(repository.mcpServers(host.id).single().enabled)
        } finally {repository.saveMcpServers(host.id,original)}
    }
}
