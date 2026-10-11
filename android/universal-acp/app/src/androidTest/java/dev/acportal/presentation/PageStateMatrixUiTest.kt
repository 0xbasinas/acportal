package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.InputModeManager
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import dev.acportal.transport.*
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.runners.Parameterized

/**
 * Loading, empty, offline and error page states in dark and light themes at default and
 * large (200%) font, using production composables only. Each page also checks what a
 * screen reader needs: every actionable node has a spoken label. Native TalkBack
 * navigation remains a device check (runbook §4).
 */
@RunWith(Parameterized::class)
class PageStateMatrixUiTest(private val mode:String,private val fontScale:Float) {
    companion object {
        @JvmStatic @Parameterized.Parameters(name="{0} font {1}")
        fun cases()=listOf(arrayOf<Any>("dark",1f),arrayOf<Any>("light",1f),arrayOf<Any>("dark",2f),arrayOf<Any>("light",2f))
    }
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    private lateinit var inputMode:InputModeManager

    private fun show(content:@Composable ()->Unit) = compose.setContent {
        val density=LocalDensity.current
        inputMode=LocalInputModeManager.current
        CompositionLocalProvider(LocalDensity provides Density(density.density,fontScale)) {
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {content()}}
        }
    }
    /** Every node a screen reader can activate must announce something. */
    private fun assertActionsAreLabelled() {
        val unlabeled=compose.onAllNodes(hasClickAction(),useUnmergedTree=false).fetchSemanticsNodes().filter {node->
            val config=node.config
            val text=config.getOrNull(SemanticsProperties.Text)?.joinToString("").orEmpty()
            val description=config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString("").orEmpty()
            val action=config.getOrNull(SemanticsActions.OnClick)?.label.orEmpty()
            text.isBlank() && description.isBlank() && action.isBlank()
        }
        assertTrue("Unlabelled actions: ${unlabeled.map {it.id}}",unlabeled.isEmpty())
    }

    @Test fun sessionListEmptyStatesAreDistinct() {
        var ui by mutableStateOf(dev.acportal.storage.StoredUiState())
        show {key(ui) {SessionsScreen(emptyList(),emptyList(),{},{},{},{},{},initialUiState=ui)}}
        compose.onNodeWithText("No sessions yet").assertIsDisplayed()
        assertActionsAreLabelled()
        compose.runOnIdle {ui=dev.acportal.storage.StoredUiState(sessionsArchived=true)}
        compose.onNodeWithText("No archived sessions").assertIsDisplayed()
        compose.runOnIdle {ui=dev.acportal.storage.StoredUiState(sessionsQuery="no such session")}
        compose.onNodeWithText("No matching sessions").assertIsDisplayed()
    }

    @Test fun noHostsOffersPairing() {
        show {HostsScreen(emptyList(),{},{},{})}
        compose.onNodeWithText("Pair your computer to start a coding session.").assertExists()
        assertActionsAreLabelled()
    }

    @Test fun loadingThenUnavailableHostKeepsRefreshReachable() {
        var loading by mutableStateOf(true)
        var reloads=0
        show {HostDetailsScreen(null,{},{},{},{reloads++},{},loading=loading)}
        compose.onNodeWithText("Connecting to host").assertIsDisplayed()
        compose.runOnIdle {loading=false}
        compose.onNodeWithText("Host unavailable").assertIsDisplayed()
        compose.onNodeWithContentDescription("Refresh host").assertHasClickAction().performClick()
        compose.runOnIdle {assertEquals(1,reloads)}
        assertActionsAreLabelled()
    }

    @Test fun errorBannerOffersRetryAndDismiss() {
        var retries=0;var dismissed=0
        show {ConnectionError("The host is offline. Check its address and try again.",{dismissed++},{retries++})}
        compose.onNodeWithText("Retry").assertIsDisplayed().performClick()
        compose.onNodeWithContentDescription("Dismiss error").performClick()
        compose.runOnIdle {assertEquals(1,retries);assertEquals(1,dismissed)}
        assertActionsAreLabelled()
    }

    @Test fun offlineSavedConversationWithoutMessagesExplainsCaching() {
        val info=SessionInfo("offline","goose","acp-offline","/workspace/offline",status="ready")
        val stored=StoredSession("h","offline",WireJson.encodeToString(info),"")
        show {SavedSessionScreen(info,stored,null,false,{},{},{})}
        compose.onNodeWithText("No messages saved").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Connect to host").assertHasClickAction()
        assertActionsAreLabelled()
    }

    @Test fun connectionLogsFailureIsShownAndCopyCanTakeFocus() {
        val events=listOf(connectionDiagnostic(ConnectionState.Connected,0),connectionDiagnostic(ConnectionState.Failed("private fixture detail"),1))
        show {ConnectionLogsScreen(events,{},"Workstation",ConnectionState.Failed("private fixture detail"))}
        val list=compose.onNodeWithTag("connection-log-list")
        list.performScrollToNode(hasText("Connection unavailable. Review connection status."))
        compose.onNodeWithText("Connection unavailable. Review connection status.").assertIsDisplayed()
        compose.onNodeWithText("private fixture detail").assertDoesNotExist()
        val copy=compose.onNode(hasText("Copy") and hasClickAction())
        copy.assertIsEnabled()
        if(copy.fetchSemanticsNode().config.contains(SemanticsActions.RequestFocus)) {
            compose.runOnIdle {assertTrue(inputMode.requestInputMode(InputMode.Keyboard))}
            copy.performSemanticsAction(SemanticsActions.RequestFocus)
            copy.assertIsFocused()
        }
        compose.onNodeWithContentDescription("Back").assertHasClickAction()
        assertActionsAreLabelled()
    }
}
