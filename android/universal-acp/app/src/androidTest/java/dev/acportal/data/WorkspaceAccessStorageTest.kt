package dev.acportal.data

import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.storage.SettingsStore
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

class WorkspaceAccessStorageTest {
    @Test fun savedPolicyReopensWithoutChangingStoredSessionAccess() = runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val repository=(context.applicationContext as PortalApplication).repository
        val stored=repository.sessions.first().firstOrNull()
        assumeTrue("Requires a paired session fixture",stored!=null)
        val info=WireJson.decodeFromString<SessionInfo>(stored!!.metadata)
        val original=repository.workspaceAccess(stored.hostId,info.workspace)
        val current=repository.sessionAccess(stored.hostId,stored.id)
        try {
            val saved=WorkspaceAccess(false,false,false)
            repository.saveWorkspaceAccess(stored.hostId,info.workspace,saved)
            assertEquals(saved,SettingsStore(context).workspacePolicies.first()[workspacePolicyKey(stored.hostId,info.workspace)])
            assertEquals(saved,repository.workspaceAccess(stored.hostId,info.workspace))
            assertEquals(current,repository.sessionAccess(stored.hostId,stored.id))
        } finally {repository.saveWorkspaceAccess(stored.hostId,info.workspace,original)}
    }
}
