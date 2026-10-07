package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.protocol.AgentInfo
import org.junit.Rule
import org.junit.Test

class AgentDiscoveryUiTest {
    @get:Rule val compose=createComposeRule()

    @Test fun missingExecutableCanBeInspectedButCannotLaunch() {
        compose.setContent {PortalTheme {RegistryAgentDetailsScreen(AgentInfo("custom","Custom agent",installed=false),"My computer",false,{},{},{error("Must not launch")})}}
        compose.onNodeWithText("Not installed").assertIsDisplayed()
        compose.onNodeWithText("New session").assertIsNotEnabled()
        compose.onNodeWithText("Supported").assertDoesNotExist()
        compose.onNodeWithText("Start a session to see the capabilities, models and sign-in methods reported by this agent.").assertExists()
    }

    @Test fun disabledInstalledAgentCannotLaunch() {
        compose.setContent {PortalTheme {RegistryAgentDetailsScreen(AgentInfo("goose","Goose",enabled=false,installed=true,status="available"),"My computer",false,{},{},{error("Must not launch")})}}
        compose.onNodeWithText("Disabled").assertIsDisplayed()
        compose.onNodeWithText("New session").assertIsNotEnabled()
        compose.onNodeWithText("Enable this agent in the computer's registry to start a session.").assertExists()
    }
    @Test fun configuredExecutableDoesNotBypassReportedConfigurationFailure() {
        compose.setContent {PortalTheme {RegistryAgentDetailsScreen(AgentInfo("custom","Custom agent",installed=true,status="misconfigured",executable="/usr/bin/custom",reason="Working directory is unavailable"),"My computer",false,{},{},{error("Must not launch")})}}
        compose.onNodeWithText("/usr/bin/custom").assertIsDisplayed()
        compose.onNodeWithText("Working directory is unavailable").assertIsDisplayed()
        compose.onNodeWithText("Version").assertDoesNotExist()
        compose.onNodeWithText("New session").assertIsNotEnabled()
    }
    @Test fun availableAgentShowsOnlyVersionActuallyReportedByHost() {
        compose.setContent {PortalTheme {RegistryAgentDetailsScreen(AgentInfo("custom","Custom agent",installed=true,status="available",executable="/usr/bin/custom",version="2.0"),"My computer",false,{},{},{})}}
        compose.onNodeWithText("/usr/bin/custom").assertIsDisplayed()
        compose.onNodeWithText("2.0").assertIsDisplayed()
        compose.onNodeWithText("Configuration problem").assertDoesNotExist()
        compose.onNodeWithText("New session").assertIsEnabled()
    }
    @Test fun savedAvailabilityCannotLaunchUntilTheHostIsRefreshed() {
        compose.setContent {PortalTheme {RegistryAgentDetailsScreen(AgentInfo("custom","Custom agent",installed=true,status="running"),"My computer",false,{},{},{error("Saved status must not launch")},savedAt=1234)}}
        compose.onNodeWithText("Saved agent list",substring=true).assertIsDisplayed()
        compose.onNodeWithText("Last reported availability").assertIsDisplayed()
        compose.onNodeWithText("New session").assertIsNotEnabled()
        compose.onNodeWithContentDescription("Refresh agent").assertIsEnabled()
    }
}
