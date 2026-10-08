package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.junit4.StateRestorationTester
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import dev.acportal.data.LiveSession
import dev.acportal.storage.HostProfile
import dev.acportal.storage.StoredSession
import dev.acportal.transport.AgentTransport
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.flow.emptyFlow
import kotlinx.coroutines.flow.flowOf
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

/** Isolated component checks: no repository, credentials, network or device-setting writes. */
class ShellApprovalLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    private val line="printf '%s\\n' '"+"review every part of this exact command ".repeat(90)+"END-OF-COMMAND'"
    private val allow="Run this exact shell line once on the selected workstation"
    private fun permission()=Permission(JsonPrimitive("shell-layout"),buildJsonObject {
        putJsonObject("_meta") {
            put("acpdSource","host-shell-command")
            putJsonObject("acpdAutoReview") {put("decision","ask");put("layer","rules");put("reason","terminal environment overrides require manual review")}
        }
        putJsonObject("toolCall") {
            put("title","Review the project command before execution")
            putJsonObject("rawInput") {
                put("shellLine",line);put("shell","/bin/sh -c");put("cwd","/workspace/"+"long-project-name/".repeat(5))
                putJsonArray("environmentNames") {add("PATH");add("PROJECT_CONFIGURATION_NAME")}
            }
        }
        putJsonArray("options") {
            add(buildJsonObject {put("optionId","allow-exact");put("name",allow);put("kind","allow_once")})
            add(buildJsonObject {put("optionId","reject-exact");put("name","Deny");put("kind","reject_once")})
        }
    })

    @Test fun compactDarkCommandAndExplicitChoicesRemainReachable()=checkPermission("dark")
    @Test fun compactLightCommandAndExplicitChoicesRemainReachable()=checkPermission("light")
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    private fun checkPermission(mode:String) {
        var enabled by mutableStateOf(false)
        val selected=mutableListOf<String>()
        val background=java.util.concurrent.atomic.AtomicReference(Color.Unspecified)
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {
                    val colour=MaterialTheme.colorScheme.surfaceContainerHigh
                    SideEffect {background.set(colour)}
                    Surface(Modifier.width(320.dp).height(280.dp).testTag("shell-window")) {
                    Column(Modifier.verticalScroll(rememberScrollState()).testTag("shell-sheet")) {
                        PermissionRequest(permission(),enabled) {selected.add(it)}
                    }
                }}
            }
        }
        compose.onNodeWithText("Host permission needed").assert(SemanticsMatcher.keyIsDefined(SemanticsProperties.Heading))
        val command=compose.onNodeWithTag("shell-command-line",useUnmergedTree=true)
        command.assertTextEquals(line)
        val layouts=mutableListOf<androidx.compose.ui.text.TextLayoutResult>()
        command.performSemanticsAction(SemanticsActions.GetTextLayoutResult) {it(layouts)}
        val foreground=layouts.single().layoutInput.style.color.luminance()
        val backdrop=background.get().luminance()
        val contrast=(maxOf(foreground,backdrop)+0.05)/(minOf(foreground,backdrop)+0.05)
        assertTrue("Shell text contrast must be at least 4.5:1, was $contrast",contrast>=4.5)
        command.performScrollTo().assertIsDisplayed().performTouchInput {swipeUp()}
        command.performSemanticsAction(SemanticsActions.ScrollBy) {scroll->assertTrue(scroll(0f,100_000f))}
        compose.waitForIdle()
        val scrollRange=command.fetchSemanticsNode().config[SemanticsProperties.VerticalScrollAxisRange]
        assertTrue("Long shell line must have scrollable content",scrollRange.maxValue()>0f)
        assertEquals("The complete command can be scrolled to its end",scrollRange.maxValue(),scrollRange.value(),1f)
        command.assertTextEquals(line)
        compose.onNodeWithText("Environment: PATH, PROJECT_CONFIGURATION_NAME").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText(allow).performScrollTo().assertIsDisplayed().assertIsNotEnabled()
        compose.onNodeWithText("Deny").performScrollTo().assertIsDisplayed().assertIsNotEnabled()
        compose.runOnIdle {assertTrue(selected.isEmpty());enabled=true}
        for ((label,id) in listOf(allow to "allow-exact","Deny" to "reject-exact")) {
            val button=compose.onNodeWithText(label).performScrollTo().assertIsDisplayed().assertIsEnabled()
                .assert(SemanticsMatcher.expectValue(SemanticsProperties.Role,Role.Button))
            val bounds=button.getUnclippedBoundsInRoot()
            val window=compose.onNodeWithTag("shell-window").getUnclippedBoundsInRoot()
            assertTrue("Decision exceeds window width",bounds.left>=window.left && bounds.right<=window.right)
            assertTrue("Decision has less than 48 dp height",bounds.bottom-bounds.top>=48.dp)
            button.performClick()
            compose.runOnIdle {assertEquals(id,selected.last())}
        }
        compose.runOnIdle {assertEquals(listOf("allow-exact","reject-exact"),selected)}
    }

    @Test fun darkAutoReviewExpansionRestoresAndKeepsFullDecisionText()=checkTool("dark")
    @Test fun lightAutoReviewExpansionRestoresAndKeepsFullDecisionText()=checkTool("light")
    private fun checkTool(mode:String) {
        val restoration=StateRestorationTester(compose)
        val reason="terminal environment overrides require manual review"
        val tool=TimelineItem.Tool("review-tool","Run project tests","execute","pending",autoReview=buildJsonObject {
            put("decision","ask");put("layer","rules");put("reason",reason);put("shellLine","printf 'FULL-COMMAND-END'")
        })
        restoration.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,2f)) {
                PortalTheme(mode) {Surface(Modifier.width(320.dp).height(280.dp)) {
                    Column(Modifier.verticalScroll(rememberScrollState()).testTag("tool-sheet")) {ToolCard(tool)}
                }}
            }
        }
        compose.onNodeWithTag("auto-review-badge",useUnmergedTree=true).assertTextEquals("Sent to you by rules review")
        val disclosure=compose.onNodeWithTag("tool-disclosure-review-tool")
        disclosure.performScrollTo().assertIsDisplayed()
            .assertContentDescriptionEquals("Expand tool: Run project tests")
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription,"Collapsed"))
        val bounds=disclosure.getUnclippedBoundsInRoot()
        assertTrue("Tool disclosure must be at least 48 dp",bounds.right-bounds.left>=48.dp && bounds.bottom-bounds.top>=48.dp)
        disclosure.performClick()
        compose.onNodeWithText("Reason: $reason").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("printf 'FULL-COMMAND-END'").performScrollTo().assertIsDisplayed()
        restoration.emulateSavedInstanceStateRestore()
        compose.onNodeWithTag("tool-disclosure-review-tool").performScrollTo()
            .assertContentDescriptionEquals("Collapse tool: Run project tests")
            .assert(SemanticsMatcher.expectValue(SemanticsProperties.StateDescription,"Expanded"))
            .performClick()
        compose.onNodeWithText("Reason: $reason").assertDoesNotExist()
    }

    @Test fun darkNativeKeyboardSheetDismissalAndReplayRequireExplicitDecision()=checkNativeSheet("dark")
    @Test fun lightNativeKeyboardSheetDismissalAndReplayRequireExplicitDecision()=checkNativeSheet("light")
    @OptIn(ExperimentalLayoutApi::class)
    private fun checkNativeSheet(mode:String) {
        val keyboard=java.util.concurrent.atomic.AtomicBoolean(false)
        val info=SessionInfo("sheet","fixture-agent","agent-session","/workspace")
        val transport=object:AgentTransport {
            override suspend fun connect() {error("No fixture connection")}
            override suspend fun disconnect() {}
            override suspend fun send(message:ByteArray) {error("No automatic fixture submissions")}
            override fun messages()=emptyFlow<ByteArray>()
            override fun connectionState()=flowOf(ConnectionState.Connected)
        }
        val live=LiveSession(HostProfile("fixture-host","Fixture host","https://example.invalid","fixture-alias","fixture-device",0),info,transport,SessionState())
        live.connection.value=ConnectionState.Connected
        val decisions=mutableListOf<String>()
        var draft="Unsent fixture draft"
        compose.setContent {
            val visible=WindowInsets.isImeVisible
            SideEffect {keyboard.set(visible)}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {
                SessionScreen(live,StoredSession("fixture-host","sheet",WireJson.encodeToString(SessionInfo.serializer(),info),draft=draft),
                    onBack={},onPrompt={_,_->error("Never send")},onCancel={},onPermission={_,option->
                        decisions.add(option);live.state.value=live.state.value.copy(permissions=emptyMap())
                    },onConfigure={_,_->error("Never configure")},onDraft={draft=it})
            }}
        }
        compose.onNode(hasSetTextAction()).performClick()
        compose.waitUntil(10_000) {keyboard.get()}
        compose.runOnIdle {live.state.value=live.state.value.copy(replaying=true,permissions=mapOf("shell-layout" to permission()))}
        compose.onNodeWithText(allow).performScrollTo().assertIsDisplayed().assertIsNotEnabled()
        compose.onNodeWithText("Deny").performScrollTo().assertIsDisplayed().assertIsNotEnabled()
        compose.runOnIdle {assertTrue(decisions.isEmpty())}
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.waitUntil(10_000) {!keyboard.get()}
        androidx.test.espresso.Espresso.pressBack()
        compose.onNodeWithText("Review permission").assertIsDisplayed()
        compose.runOnIdle {
            assertTrue(decisions.isEmpty());assertEquals(1,live.state.value.permissions.size)
            live.state.value=live.state.value.copy(replaying=false)
        }
        compose.onNodeWithText("Review permission").performClick()
        compose.onNodeWithTag("shell-command-line",useUnmergedTree=true).performScrollTo().assertTextEquals(line)
        val instrumentation=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val screenshot=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("ui-accessibility"),"shell-sheet-$mode.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
        screenshot.recycle()
        compose.onNodeWithText("Deny").performScrollTo().assertIsEnabled().performClick()
        compose.runOnIdle {
            assertEquals(listOf("reject-exact"),decisions)
            assertTrue(live.state.value.permissions.isEmpty())
            assertEquals("Unsent fixture draft",draft)
        }
    }
}
