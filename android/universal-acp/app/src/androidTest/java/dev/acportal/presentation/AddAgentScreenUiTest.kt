package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.AgentClient
import dev.acportal.data.AgentRegistration
import dev.acportal.protocol.AgentCheck
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AddAgentScreenUiTest {
    @get:Rule val compose=createComposeRule()

    private class Fake(val installed:Boolean):AgentClient {
        val checked=mutableListOf<String>();var saved:AgentRegistration?=null
        override suspend fun check(command:String,env:Map<String,String>):AgentCheck {checked+=command;return if(installed)AgentCheck(true,"/usr/bin/$command",true,"goose 1.53.0") else AgentCheck(false)}
        override suspend fun add(registration:AgentRegistration) {saved=registration}
        override suspend fun remove(id:String) {}
    }
    private fun run(theme:String,fake:Fake,onSaved:()->Unit={}) {
        compose.setContent {PortalTheme(theme) {AddAgentScreen(fake,"Laptop",{},onSaved)}}
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
    @Test fun darkCheckThenSave()=check("dark")
    @Test fun lightCheckThenSave()=check("light")
    @Test fun darkMissingProgramCannotBeSaved()=notFound("dark")
    @Test fun lightMissingProgramCannotBeSaved()=notFound("light")
}
