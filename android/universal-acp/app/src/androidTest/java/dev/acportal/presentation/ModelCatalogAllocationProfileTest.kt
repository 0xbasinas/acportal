package dev.acportal.presentation

import android.os.Bundle
import android.os.Debug
import android.os.SystemClock
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.RuleChain
import org.junit.rules.TestRule
import org.junit.runners.model.Statement

/** Measure repeated real picker interaction without provider requests or production storage. */
class ModelCatalogAllocationProfileTest {
    private val compose = createComposeRule()
    private val ownedPackage = TestRule { base, _ -> object : Statement() {
        override fun evaluate() {
            assumeTrue(InstrumentationRegistry.getInstrumentation().targetContext.packageName == "dev.acportal.acceptance")
            base.evaluate()
        }
    } }
    @get:Rule val rules: RuleChain = RuleChain.outerRule(ownedPackage).around(compose)
    @Test fun repeatedSevenHundredModelSearchAndCancelReportsAllocations() {
        val state = SessionState(models=buildJsonObject {
            put("currentModelId", "model-0")
            putJsonArray("availableModels") { repeat(700) { index -> add(buildJsonObject {
                put("modelId", "model-$index"); put("name", "Model $index")
            }) } }
        })
        var mutations = 0
        compose.setContent { PortalTheme("dark") {
            SessionOptionsScreen(SessionInfo("fixture", "agent", "acp", "/fixture"), state, true, {}, { _, _ -> mutations++ })
        } }
        compose.waitForIdle()
        fun heap() = Runtime.getRuntime().totalMemory() - Runtime.getRuntime().freeMemory()
        System.gc(); SystemClock.sleep(200)
        val initialHeap = heap()
        val allocated = Debug.getRuntimeStat("art.gc.bytes-allocated").toLong()
        val freed = Debug.getRuntimeStat("art.gc.bytes-freed").toLong()
        var peak = initialHeap
        repeat(20) {
            compose.onNodeWithText("Model 0").performClick()
            compose.onNodeWithText("Options: 700").assertIsDisplayed()
            assertTrue("Catalog must stay lazy", compose.onAllNodes(hasText("Model ", substring=true)).fetchSemanticsNodes().size < 40)
            compose.onNode(hasSetTextAction()).performTextInput("model-699")
            compose.onNodeWithText("Options: 1").assertIsDisplayed()
            compose.onNodeWithText("Model 699").assertIsDisplayed()
            peak = maxOf(peak, heap())
            compose.onNodeWithText("Cancel").performClick()
            compose.onNodeWithTag("config-choice-list").assertDoesNotExist()
            compose.runOnIdle { assertEquals(0, mutations) }
        }
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        System.gc(); SystemClock.sleep(200)
        InstrumentationRegistry.getInstrumentation().sendStatus(2, Bundle().apply {
            putString("acportalAllocationProfile", "700 models / 20 search-cancel cycles; retainedHeapDelta=${heap()-initialHeap}; sampledPeakDelta=${peak-initialHeap}; artAllocatedBytes=${Debug.getRuntimeStat("art.gc.bytes-allocated").toLong()-allocated}; artFreedBytes=${Debug.getRuntimeStat("art.gc.bytes-freed").toLong()-freed}; framework/test overhead included")
        })
    }
}
