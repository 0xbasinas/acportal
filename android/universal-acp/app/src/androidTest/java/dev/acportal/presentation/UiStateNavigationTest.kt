package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Run prepare, stop the app process, then run restore in fresh instrumentation. */
class UiStateNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    private val repository get()=(InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication).repository
    private val fixture get()=java.io.File(compose.activity.cacheDir,"ui-state-test.properties")
    @Test fun prepareDurablePageAndFiltersThroughActualNavigation() {
        val saved=runBlocking {repository.uiState.first()}
        if(!fixture.exists())java.util.Properties().apply {
            setProperty("page",saved.mainPage);setProperty("archived",saved.sessionsArchived.toString());setProperty("active",saved.sessionsActive.toString());setProperty("query",saved.sessionsQuery)
            fixture.outputStream().use {store(it,"Previous page preferences for test cleanup")}
        }
        compose.waitUntil(20_000) {compose.onAllNodesWithContentDescription("Sessions tab").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithContentDescription("Sessions tab").performClick()
        compose.onNode(hasSetTextAction()).performTextReplacement("durable-ui-check")
        val current=runBlocking {repository.uiState.first()}
        if(!current.sessionsArchived) {
            compose.onNodeWithContentDescription("Session list actions").performClick()
            compose.onNodeWithText("Archived sessions").performClick()
        }
        compose.onNodeWithText("Active",useUnmergedTree=true).performClick()
        compose.waitForIdle()
        try {compose.waitUntil(10_000) {runBlocking {repository.uiState.first().let {it.mainPage=="sessions" && it.sessionsArchived && it.sessionsActive && it.sessionsQuery=="durable-ui-check"}}}}
        catch(failure:Throwable) {throw AssertionError("Saved preferences: ${runBlocking {repository.uiState.first()}}",failure)}
        compose.onNodeWithContentDescription("Settings tab").performClick()
        compose.onNodeWithContentDescription("Sessions tab").performClick()
        compose.onNodeWithText("Archived sessions").assertIsDisplayed()
        compose.onNode(hasSetTextAction()).assertTextEquals("durable-ui-check")
        compose.waitUntil(10_000) {runBlocking {repository.uiState.first().mainPage=="sessions"}}
    }
    @Test fun restoreDurablePageAndFiltersAfterProcessStop() {
        assertTrue("Run prepare before this fresh-process test",fixture.exists())
        try {
            compose.waitUntil(20_000) {compose.onAllNodesWithText("Archived sessions").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithContentDescription("Sessions tab").assertIsSelected()
            compose.onNode(hasSetTextAction()).assertTextEquals("durable-ui-check")
            val saved=runBlocking {repository.uiState.first()}
            assertEquals("sessions",saved.mainPage);assertTrue(saved.sessionsArchived);assertTrue(saved.sessionsActive)
            val vm=androidx.lifecycle.ViewModelProvider(compose.activity)[PortalViewModel::class.java]
            compose.runOnIdle {assertNull(vm.ui.value.live)}
        } finally {
            val previous=java.util.Properties().apply {fixture.inputStream().use(::load)}
            runBlocking {repository.saveMainPage(previous.getProperty("page"));repository.saveSessionFilters(previous.getProperty("archived").toBoolean(),previous.getProperty("active").toBoolean(),previous.getProperty("query"))}
            fixture.delete()
        }
    }
}
