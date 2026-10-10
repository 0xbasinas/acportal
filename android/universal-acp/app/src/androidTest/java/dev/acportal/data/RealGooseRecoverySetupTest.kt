package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.*
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.net.URL

/** Separate preparation only. Bootstrap holds a disposable fixture token, never provider credentials. */
class RealGooseRecoverySetupTest {
    @Test fun cleanRecordedOwnedSessions() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(context.packageName == "dev.acportal.acceptance")
        assumeTrue(InstrumentationRegistry.getArguments().getString("realCleanup") == "true")
        val database = PortalDatabase.open(context)
        try {
            val hosts = database.portal().hosts().first()
            assertEquals(listOf("real-goose-fixture"), hosts.map { it.id })
            assertEquals("Real Goose fixture", hosts.single().label)
            database.portal().deleteHost(hosts.single().id)
            CredentialVault(context).remove(hosts.single().credentialAlias)
        } finally { database.close() }
    }
    @Test fun prepareTwoOwnedRealSessions() = runBlocking {
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        assumeTrue(context.packageName == "dev.acportal.acceptance")
        val port = InstrumentationRegistry.getArguments().getString("realBootstrapPort")?.toIntOrNull()
        assumeTrue("Select this fixture explicitly", port != null && port in 1024..65535)
        val bootstrap = try {
            val connection = URL("http://127.0.0.1:$port/bootstrap").openConnection().apply {
                connectTimeout = 5000
                readTimeout = 5000
            }
            WireJson.parseToJsonElement(connection.getInputStream().use { it.readBytes().toString(Charsets.UTF_8) }).objectValue()
        } catch (_: Exception) {
            // Bootstrap contains a disposable token; never attach its payload to diagnostics.
            throw IllegalStateException("Owned recovery bootstrap unavailable")
        }
        val database = PortalDatabase.open(context)
        try {
            assertTrue("Use a fresh owned acceptance installation", database.portal().hosts().first().isEmpty())
            val alias = CredentialVault(context).save(bootstrap["token"].text())
            database.portal().saveHost(HostProfile("real-goose-fixture", "Real Goose fixture", bootstrap["address"].text(), alias, "fixture", Long.MAX_VALUE, online=true))
            bootstrap["sessions"].arrayValue().forEachIndexed { index, element ->
                val info = WireJson.decodeFromJsonElement(SessionInfo.serializer(), element)
                val state = SessionState(items=listOf(TimelineItem.Text("cached-$index", "user", "Owned cached recovery ${index+1}")),
                    sessionInfo=buildJsonObject { put("title", "Real recovery ${index+1}") })
                database.portal().saveSession(StoredSession("real-goose-fixture", info.id, WireJson.encodeToString(info), WireJson.encodeToString(state), "Owned real draft ${index+1}"))
            }
            database.portal().saveMainPage("sessions")
        } finally { database.close() }
    }
}
