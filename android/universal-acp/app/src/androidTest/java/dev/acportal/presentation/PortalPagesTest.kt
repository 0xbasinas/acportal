package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import dev.acportal.MainActivity
import android.graphics.Bitmap
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.junit.Rule
import org.junit.Test

class PortalPagesTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun mainPagesAndPairFormAreReachable() {
        compose.onAllNodesWithText("Connections").onFirst().assertIsDisplayed()
        capture("hosts")
        compose.onNodeWithText("Sessions").performClick()
        compose.onNodeWithText("All sessions").assertIsDisplayed()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Appearance").assertIsDisplayed()
        compose.onNodeWithText("Appearance").performClick()
        compose.onNode(hasText("Dark") and hasAnyAncestor(isDialog())).performClick()
        compose.waitForIdle()
        capture("settings-dark")
        compose.onNodeWithContentDescription("Connections tab").performClick()
        compose.onNodeWithText("Add connection").performClick()
        compose.onNodeWithText("Address").performTextInput("https://dev.example.com:8765")
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Pairing code"))
        compose.onNodeWithText("Pairing code").performTextInput("ABCD-EFGH-IJKL")
        androidx.test.espresso.Espresso.closeSoftKeyboard()
        compose.onNode(hasScrollAction()).performScrollToNode(hasText("Pair connection"))
        compose.onNodeWithText("Pair connection").assertIsEnabled()
        capture("pair-dark")
        compose.onNodeWithContentDescription("Back").performClick()
        compose.onAllNodesWithText("Connections").onFirst().assertIsDisplayed()
    }
    private fun capture(name:String) {
        compose.waitForIdle()
        val directory=compose.activity.getExternalFilesDir("screenshots")!!
        directory.mkdirs()
        val bitmap=InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot()
        val file=File(directory,"$name.png")
        file.outputStream().use {bitmap.compress(Bitmap.CompressFormat.PNG,100,it)}
        bitmap.recycle()
        InstrumentationRegistry.getInstrumentation().uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
