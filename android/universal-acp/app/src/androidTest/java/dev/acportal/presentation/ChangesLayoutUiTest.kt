package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.FileChange
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ChangesLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun compactDarkLargeTextDiffKeepsChangesAndFileSelectionReachable()=checkDiff("dark")
    @Test fun compactLightLargeTextDiffKeepsChangesAndFileSelectionReachable()=checkDiff("light")

    private fun checkDiff(mode:String) {
        val old=(1..30).joinToString("\n") {"val line$it = $it"}
        val changed=old.replace("val line8 = 8","val line8 = 80").replace("val line24 = 24","val line24 = 240")
        val path="src/"+"long-directory/".repeat(12)+"first.kt"
        val files=listOf(FileChange("one",path,old,changed),FileChange("two","src/second.rs",null,"fn main() {}"))
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("diff-fixture")) {ChangesScreen(files,"one",{})}}
            }
        }
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Change 1 of 2").fetchSemanticsNodes().isNotEmpty()}
        val list=compose.onNode(hasScrollToIndexAction())
        val viewport=list.getUnclippedBoundsInRoot()
        assertTrue("Long path consumes the entire diff viewport",viewport.bottom-viewport.top>0.dp)
        compose.onNodeWithText("first.kt").assertIsDisplayed()
        list.performScrollToNode(hasContentDescription("Next change"))
        compose.onNodeWithContentDescription("Next change").assertIsDisplayed().performClick()
        list.performScrollToNode(hasText("val line24 = 240"))
        compose.onNodeWithText("val line24 = 240").assertIsDisplayed()
        list.performScrollToIndex(0)
        compose.onNodeWithText("Change 2 of 2").assertIsDisplayed()
        compose.onNodeWithContentDescription("Previous change").assertIsEnabled().performClick()
        list.performScrollToNode(hasText("val line8 = 80"))
        compose.onNodeWithText("val line8 = 80").assertIsDisplayed()
        compose.onNodeWithContentDescription("Choose changed file").performClick()
        compose.onNodeWithText("2. second.rs").performScrollTo().performClick()
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Change 1 of 1").fetchSemanticsNodes().isNotEmpty()}
        compose.onNode(hasScrollToIndexAction()).performScrollToNode(hasText("fn main() {}"))
        compose.onNodeWithText("fn main() {}").assertIsDisplayed()
        compose.onNodeWithContentDescription("Back").assertIsDisplayed()
    }
}
