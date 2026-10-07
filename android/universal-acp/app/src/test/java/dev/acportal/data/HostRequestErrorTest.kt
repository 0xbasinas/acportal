package dev.acportal.data

import org.junit.Assert.*
import org.junit.Test

class HostRequestErrorTest {
    @Test fun rejectedCreationOffersExplicitRecoveryWithoutClaimingTheCause() {
        val message=hostRequestError(400,"invalid_request","POST","/v1/sessions")
        assertTrue(message.contains("existing allowed folder"))
        assertTrue(message.contains("agent and MCP settings"))
        assertTrue(message.contains("press Start session again"))
        assertEquals("Host request failed (400).",hostRequestError(400,"invalid_request","GET","/v1/workspaces/browse"))
        assertEquals("Host request failed (400).",hostRequestError(400,"invalid_request","POST","/v1/pair"))
    }
    @Test fun specificTransportAndCredentialFailuresTakePrecedence() {
        assertTrue(hostRequestError(400,"unsupported_mcp_transport","POST","/v1/sessions").contains("Disable that server"))
        assertTrue(hostRequestError(401,"unauthorized","POST","/v1/sessions").contains("Pair this host again"))
        assertTrue(hostRequestError(409,"controller_conflict","POST","/v1/sessions").contains("controller"))
    }
    @Test fun rejectedPairingOffersANewCodeWithoutClaimingCredentialExpiry() {
        val message=hostRequestError(401,"pairing_failed","POST","/v1/pair")
        assertTrue(message.contains("Pairing code was rejected"))
        assertTrue(message.contains("acpd pair"))
        assertFalse(message.contains("Credentials expired"))
    }
}
