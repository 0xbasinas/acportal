package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import dev.acportal.storage.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

class SessionsLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun compactDarkFiltersRowsAndActionsRemainReachable()=checkList("dark")
    @Test fun compactLightFiltersRowsAndActionsRemainReachable()=checkList("light")
    @Test fun darkNativeSearchKeyboardRetainsActiveFilterAndCanBeDismissed()=checkKeyboard("dark")
    @Test fun lightNativeSearchKeyboardRetainsActiveFilterAndCanBeDismissed()=checkKeyboard("light")

    @OptIn(ExperimentalLayoutApi::class)
    private fun checkKeyboard(mode:String) {
        val visible=AtomicBoolean(false)
        val filters=AtomicReference(Triple(false,true,""))
        val info=SessionInfo("one","mock","acp","/projects/app",status="ready")
        val stored=StoredSession("host","one",WireJson.encodeToString(SessionInfo.serializer(),info))
        compose.setContent {
            val keyboard=WindowInsets.isImeVisible
            SideEffect {visible.set(keyboard)}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {
                SessionsScreen(listOf(stored),emptyList(),{},{},{},{},{},initialUiState=StoredUiState(sessionsActive=true),onFiltersChanged={archived,active,query->filters.set(Triple(archived,active,query))})
            }}
        }
        compose.onNode(hasSetTextAction()).performClick()
        compose.waitUntil(10_000) {visible.get()}
        compose.onNode(hasSetTextAction()).assertIsFocused().performTextInput("missing fixture")
        compose.waitUntil(10_000) {visible.get() && filters.get().third=="missing fixture"}
        val list=compose.onNodeWithTag("sessions-list")
        list.performScrollToNode(hasText("No matching sessions"))
        compose.onNodeWithText("No matching sessions").assertIsDisplayed()
        list.performScrollToNode(hasContentDescription("Clear session search"))
        compose.onNodeWithContentDescription("Clear session search").assertIsDisplayed().performClick()
        compose.waitUntil(10_000) {filters.get()==Triple(false,true,"")}
        list.performScrollToNode(hasTestTag("session-one"))
        compose.onNodeWithTag("session-one").assertIsDisplayed()
        list.performScrollToNode(hasSetTextAction())
        compose.onNode(hasSetTextAction()).performClick()
        compose.waitUntil(10_000) {visible.get()}
        compose.onNode(hasSetTextAction()).performImeAction()
        compose.waitUntil(10_000) {!visible.get()}
        compose.onNode(hasSetTextAction()).assertIsNotFocused()
        compose.onNodeWithContentDescription("Clear session search").assertDoesNotExist()
    }

    private fun checkList(mode:String) {
        val info=SessionInfo("one","mock","acp","/projects/app",status="ready")
        val stored=StoredSession("host","one",WireJson.encodeToString(SessionInfo.serializer(),info))
        var opened=0;var archived=0;var deleted=0;var newSessions=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp)) {
                    SessionsScreen(listOf(stored),emptyList(),{opened++},{archived++},{},{deleted++},{},onNew={newSessions++})
                }}
            }
        }
        val list=compose.onNodeWithTag("sessions-list")
        val viewport=list.getUnclippedBoundsInRoot()
        assertTrue("Filters consume the session list",viewport.bottom-viewport.top>0.dp)
        list.performScrollToNode(hasText("Active"));compose.onNodeWithText("Active").performClick()
        list.performScrollToNode(hasTestTag("session-one"))
        compose.onNodeWithTag("session-one").assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals(1,opened)}
        list.performScrollToNode(hasContentDescription("Session actions"))
        // Lazy-list semantics scrolls to the viewport top, which includes the pinned search field.
        val pinnedHeight=compose.onNode(hasSetTextAction()).fetchSemanticsNode().boundsInRoot.height
        list.performSemanticsAction(SemanticsActions.ScrollBy) {it(0f,-pinnedHeight)}
        val menuBounds=compose.onNodeWithContentDescription("Session actions").fetchSemanticsNode().boundsInRoot
        val searchBounds=compose.onNode(hasSetTextAction()).fetchSemanticsNode().boundsInRoot
        assertTrue("Session actions are covered by pinned search",menuBounds.top>=searchBounds.bottom)
        compose.onNodeWithContentDescription("Session actions").performClick()
        compose.onNodeWithText("Archive").performClick()
        compose.runOnIdle {assertEquals(1,archived);assertEquals(0,deleted)}
        list.performScrollToNode(hasText("Search sessions"))
        compose.onNode(hasSetTextAction()).performTextInput("missing fixture")
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        list.performScrollToNode(hasText("No matching sessions"))
        compose.onNodeWithText("No matching sessions").assertIsDisplayed()
        list.performScrollToNode(hasSetTextAction())
        compose.onNode(hasSetTextAction()).performTextClearance()
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.onNodeWithContentDescription("Session list actions").performClick()
        compose.onNodeWithText("Archived sessions").performClick()
        list.performScrollToNode(hasText("No archived sessions"))
        compose.onNodeWithText("No archived sessions").assertIsDisplayed()
        compose.onNodeWithContentDescription("New session").performClick()
        compose.runOnIdle {assertEquals(1,newSessions);assertEquals(0,deleted)}
    }
}
