package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.HostDetails
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class NewSessionLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun shortWindowWithLargeTextKeepsChoicesAndActionsReachable()=checkChoices(true)
    @Test fun landscapeKeepsChoicesAndActionsReachable() {
        val context=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation().targetContext
        org.junit.Assume.assumeTrue("Run on the landscape emulator",context.resources.configuration.orientation==android.content.res.Configuration.ORIENTATION_LANDSCAPE)
        checkChoices(false)
    }
    private fun checkChoices(shortWindow:Boolean) {
        val host=HostProfile("h","Host","https://host.example","alias","d",0)
        val details=HostDetails(host,listOf(AgentInfo("one","First agent",installed=true,status="available"),AgentInfo("two","Second agent",installed=true,status="available")),listOf(Workspace("/workspace","Project workspace")),emptyList(),emptyList())
        var selected="";var workspace="";var access=0;var mcp=0
        compose.setContent {
            val density=LocalDensity.current
            CompositionLocalProvider(LocalDensity provides Density(density.density,if(shortWindow)2f else 1f)) {
                PortalTheme("Dark") {Box(if(shortWindow)Modifier.width(320.dp).height(280.dp) else Modifier.fillMaxSize()) {androidx.compose.material3.Surface {NewSessionScreen(details,null,false,{}, {_,_->},{agent,path->selected=agent;workspace=path},onMcp={mcp++},onAccess={access++})}}}
            }
        }
        compose.onNodeWithTag("new-session-choices").performScrollToNode(hasTestTag("agent-choice:two"))
        compose.onNodeWithTag("agent-choice:two").assertIsDisplayed().performClick()
        compose.onNodeWithTag("new-session-choices").performScrollToNode(hasText("Workspace access"))
        compose.onNodeWithText("Workspace access").assertIsDisplayed().performClick()
        compose.onNodeWithTag("new-session-choices").performScrollToNode(hasText("MCP servers"))
        compose.onNodeWithText("MCP servers").assertIsDisplayed().performClick()
        compose.onNodeWithTag("new-session-choices").performScrollToNode(hasText("Start session"))
        compose.onNodeWithText("Start session").assertIsDisplayed().assertIsEnabled().performClick()
        compose.runOnIdle {assertEquals("two",selected);assertEquals("/workspace",workspace);assertEquals(1,access);assertEquals(1,mcp)}
        if(!shortWindow) {
            val instrumentation=androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
            val screenshot=instrumentation.uiAutomation.takeScreenshot()
            val file=java.io.File(instrumentation.targetContext.getExternalFilesDir("screenshots"),"chooser-landscape.png")
            file.parentFile!!.mkdirs();file.outputStream().use {screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};screenshot.recycle()
            instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-chooser-landscape.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
        }
    }
}
