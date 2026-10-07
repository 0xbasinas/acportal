package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.storage.PortalSettings
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SettingsLayoutUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(45)
    @Test fun darkAppearanceAndHistoryRemainReachable()=checkSettings("dark")
    @Test fun lightAppearanceAndHistoryRemainReachable()=checkSettings("light")
    private fun checkSettings(mode:String) {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val configuration=instrumentation.targetContext.resources.configuration
        if(InstrumentationRegistry.getArguments().getString("requireLandscape")=="true")assertEquals(android.content.res.Configuration.ORIENTATION_LANDSCAPE,configuration.orientation)
        if(InstrumentationRegistry.getArguments().getString("requireLargeFont")=="true")assertTrue(configuration.fontScale>=1.9f)
        var settings by mutableStateOf(PortalSettings(mode,true))
        var themes=0;var cacheChanges=0
        compose.setContent {PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding()) {SettingsScreen(settings,{themes++;settings=settings.copy(theme=it)},{cacheChanges++;settings=settings.copy(cacheMessages=it)})}}}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Appearance"))
        compose.onNodeWithText("Appearance").performClick()
        val dark=hasText("Dark") and hasAnyAncestor(isDialog())
        compose.onNode(hasScrollAction() and hasAnyAncestor(isDialog())).performScrollToNode(dark)
        compose.onNode(dark).assertIsDisplayed().performClick()
        compose.runOnIdle {assertEquals("dark",settings.theme);assertEquals(1,themes)}
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("History on this device"))
        compose.onNodeWithText("History on this device").performClick()
        val historyBounds=compose.onNode(hasScrollAction() and hasAnyAncestor(isDialog())).getUnclippedBoundsInRoot()
        assertTrue("History dialog needs space for an accessible control",historyBounds.bottom-historyBounds.top>=48.dp)
        compose.onNode(hasScrollAction() and hasAnyAncestor(isDialog())).performScrollToNode(hasText("Save messages"))
        compose.onNodeWithText("Save messages").assertIsDisplayed()
        val toggle=compose.onNode(isToggleable() and hasAnyAncestor(isDialog())).getUnclippedBoundsInRoot()
        val viewport=compose.onNode(hasScrollAction() and hasAnyAncestor(isDialog())).getUnclippedBoundsInRoot()
        assertTrue("The history switch must be fully reachable",toggle.top>=viewport.top && toggle.bottom<=viewport.bottom)
        compose.onNode(isToggleable() and hasAnyAncestor(isDialog())).performClick()
        capture("settings-history-$mode")
        compose.onNodeWithText("Done").assertIsDisplayed().performClick()
        compose.runOnIdle {assertFalse(settings.cacheMessages);assertEquals(1,cacheChanges)}
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
