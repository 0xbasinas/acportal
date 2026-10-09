package dev.acportal.data

import org.junit.Assert.*
import org.junit.Test

class PairingInputTest {
    private val valid="acportal://pair?address=https%3A%2F%2Fhost.example%3A8443&code=ABCD-1234-EF56&name=Work+%26+laptop"
    @Test fun hostPayloadRoundTripsWithoutRedeeming() {
        assertEquals(PairingInput("https://host.example:8443","ABCD-1234-EF56","Work & laptop"),parsePairingInput(valid,false))
    }
    @Test fun manuallyPastedSpacingAndCaseAreNormalized() {
        assertEquals("ABCD-1234-EF56",normalizePairingCode(" abcd 1234 ef56 "))
        assertEquals("ABCD-1234-EF56",normalizePairingCode("abcd-1234-ef56"))
    }
    @Test fun ambiguousAndUntrustedPayloadsAreRejectedWithoutExposingCodes() {
        val values=listOf(valid+"&code=SECRET-CODE",valid+"&token=SECRET-CODE",valid.replace("https%3A","http%3A"),valid.replace("host.example","user%3Apassword%40host.example"),valid.replace("ABCD-1234-EF56","SECRET-CODE"),valid.replace("ABCD-1234-EF56","%GGSECRET-CODE"),valid+"#SECRET-CODE",valid.replace("acportal://pair","https://pair"),"x".repeat(4097))
        values.forEach {input->val failure=runCatching {parsePairingInput(input,false)}.exceptionOrNull();assertNotNull(failure);assertFalse(failure!!.message.orEmpty().contains("SECRET-CODE"))}
    }
    @Test fun remoteCleartextRemainsRejectedInDebug() {
        assertTrue(runCatching {parsePairingInput(valid.replace("https%3A","http%3A"),true)}.isFailure)
        assertEquals("http://127.0.0.1:8443",parsePairingInput(valid.replace("https%3A%2F%2Fhost.example","http%3A%2F%2F127.0.0.1"),true).address)
    }
}
