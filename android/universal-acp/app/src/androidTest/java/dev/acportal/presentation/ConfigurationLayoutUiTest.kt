package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.McpDefinition
import dev.acportal.data.McpValue
import androidx.test.platform.app.InstrumentationRegistry
import java.util.concurrent.atomic.AtomicBoolean
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class ConfigurationLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun compactDarkEditorKeepsTransportAndSaveInsideTheWindow()=checkEditor("dark")
    @Test fun compactLightEditorKeepsTransportAndSaveInsideTheWindow()=checkEditor("light")
    @Test fun compactDarkLongDefinitionCanBeOpenedCorrectedAndSaved()=checkLongDefinition("dark")
    @Test fun compactLightLongDefinitionCanBeOpenedCorrectedAndSaved()=checkLongDefinition("light")

    private fun checkLongDefinition(mode:String) {
        val original=McpDefinition("long-fixture","Project documentation ".repeat(6).take(128),"http","https://docs.example.invalid/"+"nested-path/".repeat(150),values=listOf(McpValue("X-Fixture","v".repeat(4096))))
        var servers by mutableStateOf(listOf(original))
        var saves=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("configuration-fixture")) {McpServersScreen("Workstation",servers,false,{}, {updated,done->saves++;servers=updated;done()})}}
            }
        }
        // A long endpoint must not turn one server into an unbounded-height row.
        listOf(original.name,original.displayEndpoint()).forEach {text->
            val layouts=mutableListOf<TextLayoutResult>()
            compose.onNodeWithText(text).performSemanticsAction(SemanticsActions.GetTextLayoutResult) {it(layouts)}
            assertTrue("Long server summary exceeds two lines",layouts.single().lineCount<=2)
        }
        reveal(original.name)
        compose.onNodeWithText(original.name).assertIsDisplayed().performClick()
        reveal("Server name")
        compose.onNodeWithText("Server name").assertTextContains(original.name)
        reveal("Server URL")
        compose.onNodeWithText("Server URL").assertTextContains(original.endpoint)
        reveal("Headers")
        compose.onNodeWithText("Headers").assertTextContains("X-Fixture="+original.values.single().value)
        compose.onNodeWithText("Headers").performTextReplacement("X-Fixture="+"v".repeat(4097))
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        reveal("Save server")
        compose.onNodeWithText("Save server").performClick()
        reveal("Check the environment variable or header names and values")
        compose.onNodeWithText("Check the environment variable or header names and values").assertIsDisplayed()
        compose.runOnIdle {assertEquals(0,saves);assertEquals(listOf(original),servers)}
        reveal("Headers")
        compose.onNodeWithText("Headers").performTextReplacement("X-Fixture="+original.values.single().value)
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        reveal("Save server")
        compose.onNodeWithText("Save server").assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals(1,saves);assertEquals(listOf(original),servers)}
        reveal(original.name)
        compose.onNodeWithText(original.name).performClick()
        reveal("Headers")
        compose.onNodeWithText("Headers").assertTextContains("X-Fixture="+original.values.single().value)
    }
    @Test fun keyboardKeepsTheSaveActionReachableWithoutLosingTheDefinition()=checkKeyboard("light")
    @Test fun darkKeyboardKeepsTheSaveActionReachableWithoutLosingTheDefinition()=checkKeyboard("dark")

    @OptIn(ExperimentalLayoutApi::class)
    private fun checkKeyboard(mode:String) {
        var servers by mutableStateOf(emptyList<McpDefinition>())
        val keyboard=AtomicBoolean(false)
        compose.setContent {
            val visible=WindowInsets.isImeVisible
            SideEffect {keyboard.set(visible)}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {McpServersScreen("Workstation",servers,false,{}, {updated,done->servers=updated;done()})}}
        }
        compose.onNodeWithText("+  Add server").performTouchInput {click()}
        compose.onNodeWithText("Server name").performTouchInput {click()}
        compose.onNodeWithText("Server name").performTextInput("Project docs")
        compose.waitUntil(10_000) {keyboard.get()}
        reveal("HTTP")
        compose.onNodeWithText("HTTP").performClick()
        reveal("Server URL")
        compose.onNodeWithText("Server URL").performTouchInput {click()}
        compose.onNodeWithText("Server URL").performTextInput("https://docs.example.com/mcp")
        reveal("Save server")
        compose.onNodeWithText("Save server").assertIsDisplayed()
        assertTrue("The native keyboard must still be visible",keyboard.get())
        compose.onNodeWithText("Save server").performTouchInput {click()}
        compose.runOnIdle {assertEquals("https://docs.example.com/mcp",servers.single().endpoint);assertEquals("Project docs",servers.single().name)}
    }

    private fun checkEditor(mode:String) {
        var servers by mutableStateOf(emptyList<McpDefinition>())
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp).testTag("configuration-fixture")) {McpServersScreen("Workstation",servers,false,{}, {updated,done->servers=updated;done()})}}
            }
        }
        reveal("+  Add server")
        compose.onNodeWithText("+  Add server").performClick()
        reveal("Server name")
        compose.onNodeWithText("Server name").performTextInput("Project docs")
        // This bounded layout fixture excludes the full phone keyboard; the separate test covers real IME insets.
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.waitForIdle()
        reveal("SSE")
        val bounds=compose.onNodeWithText("SSE").getUnclippedBoundsInRoot()
        val window=compose.onNodeWithTag("configuration-fixture").getUnclippedBoundsInRoot()
        assertTrue("SSE extends outside the compact editor",bounds.right<=window.right && bounds.left>=window.left)
        val layouts=mutableListOf<TextLayoutResult>()
        compose.onNodeWithText("SSE").performSemanticsAction(SemanticsActions.GetTextLayoutResult) {it(layouts)}
        assertEquals("The SSE transport label wraps in the compact editor",1,layouts.single().lineCount)
        compose.onNodeWithText("SSE").assertIsDisplayed().performClick()
        reveal("HTTP")
        compose.onNodeWithText("HTTP").performClick()
        reveal("Server URL")
        compose.onNodeWithText("Server URL").performTextInput("https://docs.example.com/mcp")
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.waitForIdle()
        reveal("Save server")
        compose.onNodeWithText("Save server").assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals("http",servers.single().transport);assertEquals("Project docs",servers.single().name)}
    }
    private fun reveal(text:String) {
        if(compose.onAllNodes(hasText(text) and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().isNotEmpty())compose.onNode(hasScrollAction() and !hasSetTextAction()).performScrollToNode(hasText(text))
    }
    private fun capture(name:String) {
        compose.waitForIdle()
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val image=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("screenshots"),"$name.png")
        file.parentFile!!.mkdirs();file.outputStream().use {image.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};image.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
