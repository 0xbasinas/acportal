package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.AgentClient
import dev.acportal.data.AgentRegistration
import dev.acportal.protocol.AgentCheck
import dev.acportal.protocol.KnownAgent
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AddAgentScreenUiTest {
    @get:Rule val compose=createComposeRule()

    // What a host would list; the app itself has no list of agents.
    private val catalog=listOf(
        KnownAgent("goose","Goose","goose",listOf("acp"),mapOf("GOOSE_MODE" to "approve")),
        KnownAgent("gemini","Gemini CLI","gemini",listOf("--acp")),
        KnownAgent("opencode","OpenCode","opencode",listOf("acp"),ownTools=true),
    )
    private inner class Fake(val installed:Boolean,val onHost:Set<String> = setOf("goose"),var scanFailures:Int=0):AgentClient {
        val checked=mutableListOf<String>();var saved:AgentRegistration?=null;var scans=0
        override suspend fun discover():List<KnownAgent> {
            scans++
            if(scanFailures>0) {scanFailures--;error("offline")}
            return catalog.map {if(it.command in onHost)it.copy(found=true,executable="/home/me/.local/bin/${it.command}") else it}
        }
        override suspend fun check(command:String,env:Map<String,String>):AgentCheck {checked+=command;return if(installed)AgentCheck(true,"/usr/bin/$command",true,"goose 1.53.0") else AgentCheck(false)}
        override suspend fun add(registration:AgentRegistration) {saved=registration}
        override suspend fun remove(id:String) {}
    }
    private fun run(theme:String,fake:Fake,onSaved:()->Unit={}) {
        compose.setContent {PortalTheme(theme) {AddAgentScreen(fake,"Laptop",{},onSaved)}}
        // The screen first asks the host which popular agents are installed.
        compose.waitUntil(5000) {compose.onAllNodesWithTag("agent-scan-progress").fetchSemanticsNodes().isEmpty()}
    }
    private fun fill() {
        compose.onNodeWithTag("agent-name").performTextInput("Goose")
        compose.onNodeWithTag("agent-command").performTextInput("goose")
        compose.onNodeWithTag("agent-args").performTextInput("acp")
        compose.onNodeWithTag("agent-env").performTextInput("GOOSE_MODE=approve")
    }
    private fun check(theme:String) {
        val fake=Fake(true);var saved=false
        run(theme,fake) {saved=true}
        compose.onNodeWithTag("agent-check").assertIsNotEnabled()
        compose.onNodeWithTag("agent-save").assertIsNotEnabled()
        fill()
        compose.onNodeWithTag("agent-save").assertIsNotEnabled()
        compose.onNodeWithTag("agent-check").assertIsEnabled().performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithTag("agent-check-result").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("Found").assertExists()
        compose.onNodeWithText("Version: goose 1.53.0").assertExists()
        compose.onNodeWithTag("agent-save").assertIsEnabled().performClick()
        compose.waitUntil(5000) {saved}
        assertEquals(AgentRegistration("goose","Goose","goose",listOf("acp"),mapOf("GOOSE_MODE" to "approve"),false),fake.saved)
    }
    private fun notFound(theme:String) {
        val fake=Fake(false)
        run(theme,fake)
        fill()
        compose.onNodeWithTag("agent-check").performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithText("Not found on this computer").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithTag("agent-save").assertIsNotEnabled()
        // Editing the command clears the old result, so a stale pass cannot be saved.
        compose.onNodeWithTag("agent-command").performTextInput("x")
        compose.onNodeWithText("Not found on this computer").assertDoesNotExist()
        assertNull(fake.saved)
    }
    private fun suggestion(theme:String) {
        val fake=Fake(true)
        run(theme,fake)
        compose.onNodeWithTag("suggest:Goose").performClick()
        compose.onNodeWithTag("agent-name").assertTextContains("Goose")
        compose.onNodeWithTag("agent-command").assertTextContains("goose")
        compose.onNodeWithTag("agent-args").assertTextContains("acp")
        compose.onNodeWithTag("agent-env").assertTextContains("GOOSE_MODE=approve")
        // Filling the form registers nothing and does not count as a check.
        compose.onNodeWithTag("agent-save").assertIsNotEnabled()
        assertNull(fake.saved);assertTrue(fake.checked.isEmpty())
        compose.onNodeWithTag("agent-check").performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithTag("agent-check-result").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithTag("agent-save").assertIsEnabled()
        // Picking another suggestion discards the earlier check.
        compose.onNodeWithTag("suggest:Gemini CLI").performClick()
        compose.onNodeWithTag("agent-save").assertIsNotEnabled()
    }
    @Test fun darkSuggestionFillsTheFormButRegistersNothing()=suggestion("dark")
    @Test fun lightSuggestionFillsTheFormButRegistersNothing()=suggestion("light")
    @Test fun installedAgentsAreShownFirstWithTheirPathAndTheRestAreMarkedNotInstalled() {
        run("dark",Fake(true,onHost=setOf("goose","gemini")))
        compose.onNodeWithTag("agent-found").assertExists()
        compose.onNodeWithText("Found: /home/me/.local/bin/goose").assertExists()
        compose.onNodeWithText("Not installed here",substring=true).assertExists()
        compose.onNodeWithTag("agent-none-found").assertDoesNotExist()
        // Both kinds still prefill the form.
        compose.onNodeWithTag("suggest:Goose").performClick()
        compose.onNodeWithTag("agent-command").assertTextContains("goose")
        compose.onNodeWithTag("suggest:OpenCode").performClick()
        compose.onNodeWithTag("agent-command").assertTextContains("opencode")
    }
    @Test fun whenNothingIsInstalledTheScreenSaysSoAndKeepsTheOthersUsable() {
        run("light",Fake(true,onHost=emptySet()))
        compose.onNodeWithTag("agent-none-found").assertExists()
        compose.onNodeWithTag("agent-found").assertDoesNotExist()
        compose.onNodeWithTag("suggest:Goose").performClick()
        compose.onNodeWithTag("agent-name").assertTextContains("Goose")
    }
    @Test fun aFailedScanSaysSoAndScanAgainTriesAgain() {
        val fake=Fake(true,scanFailures=1)
        run("dark",fake)
        compose.onNodeWithTag("agent-scan-failed").assertExists()
        compose.onNodeWithTag("agent-scan-again").performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithTag("agent-scan-failed").fetchSemanticsNodes().isEmpty() && compose.onAllNodesWithTag("agent-scan-progress").fetchSemanticsNodes().isEmpty()}
        compose.onNodeWithTag("agent-found").assertExists()
        assertEquals(2,fake.scans)
    }
    @Test fun darkCheckThenSave()=check("dark")
    @Test fun lightCheckThenSave()=check("light")
    @Test fun darkMissingProgramCannotBeSaved()=notFound("dark")
    @Test fun lightMissingProgramCannotBeSaved()=notFound("light")
}
