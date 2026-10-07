package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.FileChange
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

class ChangesUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    private val old=(1..30).joinToString("\n") {"val line$it = $it"}
    private val changed=old.replace("val line8 = 8","val line8 = 80").replace("val line24 = 24","val line24 = 240")
    private fun awaitText(text:String) {compose.waitUntil(10_000) {compose.onAllNodesWithText(text).fetchSemanticsNodes().isNotEmpty()}}

    @Test fun navigatesModificationsAndFiles() {
        val files=listOf(FileChange("one","src/first.kt",old,changed),FileChange("two","src/second.rs",null,"fn main() {}"))
        compose.setContent {PortalTheme {ChangesScreen(files,"one",{})}}
        awaitText("Change 1 of 2")
        compose.onNodeWithText("val line8 = 80").assertExists()
        compose.onNodeWithContentDescription("Next change").performClick()
        awaitText("Change 2 of 2")
        compose.onNodeWithText("val line24 = 240").assertIsDisplayed()
        compose.onNodeWithContentDescription("Previous change").performClick()
        awaitText("Change 1 of 2")
        capture("changes-dark")
        compose.onNodeWithContentDescription("Choose changed file").performClick()
        compose.onNodeWithText("2. second.rs").performClick()
        awaitText("fn main() {}")
        compose.onNodeWithText("Change 1 of 1").assertExists()
        compose.onNodeWithContentDescription("Next change").assertIsNotEnabled()
    }

    @Test fun expandsUnchangedContentBeyondOldPreviewLimit() {
        val text=(1..1005).joinToString("\n") {"line $it"}
        compose.setContent {PortalTheme("light") {ChangesScreen(listOf(FileChange("one","notes.txt",text,text)),"one",{})}}
        awaitText("Show 1005 unchanged lines")
        compose.onNodeWithContentDescription("Next change").assertIsNotEnabled()
        compose.onNodeWithText("Show 1005 unchanged lines").performClick()
        compose.onNode(hasScrollToIndexAction()).performScrollToIndex(1004)
        compose.onNodeWithText("line 1005").assertIsDisplayed()
        capture("changes-light")
        compose.onNodeWithText("Collapse unchanged lines").performClick()
        awaitText("Show 1005 unchanged lines")
    }

    @Test fun wideWindowOffersSplitAndUnifiedViews() {
        assumeTrue("Requires a wide emulator window",InstrumentationRegistry.getArguments().getString("expectWide")=="true")
        compose.setContent {PortalTheme {ChangesScreen(listOf(FileChange("one","src/first.kt",old,changed)),"one",{})}}
        awaitText("Split view")
        compose.onNodeWithText("Split view").performClick()
        compose.onNodeWithText("Before").assertIsDisplayed()
        compose.onNodeWithText("After").assertIsDisplayed()
        capture("changes-split")
        compose.onNodeWithText("Unified").performClick()
        compose.onNodeWithText("Before").assertDoesNotExist()
    }

    private fun capture(name:String) {
        compose.waitForIdle();android.os.SystemClock.sleep(250)
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        var screenshot=instrumentation.uiAutomation.takeScreenshot()
        repeat(2) {if(screenshot==null) {android.os.SystemClock.sleep(100);screenshot=instrumentation.uiAutomation.takeScreenshot()}}
        val bitmap=requireNotNull(screenshot) {"Native screenshot unavailable after three attempts"}
        val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("screenshots"),"changes.png")
        file.parentFile!!.mkdirs();file.outputStream().use {bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};bitmap.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
