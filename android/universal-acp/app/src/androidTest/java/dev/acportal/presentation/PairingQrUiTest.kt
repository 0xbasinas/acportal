package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class PairingQrUiTest {
    @get:Rule val compose=createComposeRule()
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    private val payload="acportal://pair?address=https%3A%2F%2Fhost.example&code=ABCD-1234-EF56&name=Workstation"
    @Test fun darkScanFillsFieldsAndNeedsExplicitPair()=checkScan("dark")
    @Test fun lightScanFillsFieldsAndNeedsExplicitPair()=checkScan("light")
    private fun checkScan(theme:String) {
        var calls=0;var value=emptyList<String>()
        compose.setContent {PortalTheme(theme) {PairScreen(false,{}, {address,code,name->calls++;value=listOf(address,code,name)},scanner={result,_,_->result(payload)})}}
        compose.onNodeWithText("Scan pairing QR code").performClick()
        compose.runOnIdle {assertEquals(0,calls)}
        scroll("Address");compose.onNodeWithText("Address").assertTextContains("https://host.example")
        scroll("Pairing code");compose.onNodeWithText("Pairing code").assertTextContains("ABCD-1234-EF56")
        scroll("Pair connection");compose.onNodeWithText("Pair connection").performClick()
        compose.runOnIdle {assertEquals(1,calls);assertEquals(listOf("https://host.example","ABCD-1234-EF56","Workstation"),value)}
    }
    @Test fun cancellingScannerPreservesManualAddress() {
        compose.setContent {PortalTheme {PairScreen(false,{}, {_,_,_->fail("Automatic pairing")},initialAddress="https://original.example",scanner={_,_,cancel->cancel()})}}
        compose.onNodeWithText("Scan pairing QR code").performClick()
        scroll("Address");compose.onNodeWithText("Address").assertTextContains("https://original.example")
        scroll("Pair connection");compose.onNodeWithText("Pair connection").assertIsNotEnabled()
    }
    @Test fun invalidQrAndScannerFailureKeepManualRecovery() {
        var scans=0
        compose.setContent {PortalTheme("light") {PairScreen(false,{}, {_,_,_->fail("Automatic pairing")},scanner={result,failed,_->if(scans++==0)result("https://untrusted.example") else failed()})}}
        compose.onNodeWithText("Scan pairing QR code").performClick()
        scroll("This is not a valid ACP Portal pairing QR code. Ask the host for a fresh QR code or enter its details below.")
        scroll("Scan pairing QR code");compose.onNodeWithText("Scan pairing QR code").performClick()
        scroll("QR scanning is unavailable. Check Google Play services or enter the address and code below.")
        scroll("Address");compose.onNodeWithText("Address").assertIsEnabled()
    }
    private fun scroll(label:String) {if(label=="Pair connection")compose.onNodeWithText(label).assertIsDisplayed() else compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(label))}
}
