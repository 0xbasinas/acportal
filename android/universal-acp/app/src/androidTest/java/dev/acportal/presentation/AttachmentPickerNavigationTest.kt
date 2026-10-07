package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.protocol.*
import dev.acportal.transport.ConnectionState
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/** Select attachment-fixture.txt in the native picker while this test waits. */
class AttachmentPickerNavigationTest {
    @get:Rule val compose=createAndroidComposeRule<MainActivity>()
    @Test fun pickerPreparesAFileAndExplicitSendStartsTheTurn() {
        assumeTrue("Requires native picker verification",InstrumentationRegistry.getArguments().getString("expectNativePicker")=="true")
        val repository=(compose.activity.application as PortalApplication).repository
        val host=runBlocking {repository.hosts.first().firstOrNull()}
        assumeTrue("Requires the paired media mock host",host!=null)
        val stored=runBlocking {repository.create(host!!.id,"mock",repository.api.workspaces(host).first().path)}
        try {
            compose.onNodeWithContentDescription("Sessions tab").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithTag("session-${stored.id}").fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithTag("session-${stored.id}").performClick()
            compose.waitUntil(20_000) {compose.onAllNodesWithText("Mock sign-in").fetchSemanticsNodes().isNotEmpty()}
            compose.waitUntil(20_000) {compose.onAllNodes(hasText("Mock sign-in") and isEnabled()).fetchSemanticsNodes().isNotEmpty()}
            compose.onNodeWithText("Mock sign-in").performClick()
            val live=runBlocking {repository.open(host!!.id,stored.id)}
            runBlocking {withTimeout(20_000) {live.metadata.first {it.acpSessionId.isNotBlank()};live.connection.first {it is ConnectionState.Connected}}}
            compose.onNodeWithContentDescription("Add context").performClick()
            compose.onNodeWithText("Attach image").assertIsDisplayed()
            compose.onNodeWithText("Attach audio").assertIsDisplayed()
            capture("attachment-menu")
            compose.onNodeWithText("Attach file").performClick()
            InstrumentationRegistry.getInstrumentation().sendStatus(1,android.os.Bundle().apply {putString("stream","Native attachment picker is ready\n")})
            compose.waitUntil(120_000) {live.attachments.value.any {it.name=="attachment-fixture.txt"}}
            compose.onNodeWithText("attachment-fixture.txt").assertIsDisplayed()
            compose.onNodeWithContentDescription("Send message").assertIsEnabled()
            compose.runOnIdle {assertFalse(live.state.value.processing);assertTrue(live.state.value.items.isEmpty())}
            capture("attachment-composer")
            compose.onNodeWithContentDescription("Session menu").performClick()
            compose.onNodeWithText("Agent details").performClick()
            compose.onNodeWithContentDescription("Back").performClick()
            compose.onNodeWithText("attachment-fixture.txt").assertIsDisplayed()
            compose.onNodeWithContentDescription("Send message").performClick()
            compose.waitUntil(20_000) {live.state.value.permissions.isNotEmpty()}
            compose.runOnIdle {assertTrue(live.attachments.value.isEmpty())}
            runBlocking {repository.cancel(live);withTimeout(20_000) {live.state.first {!it.processing && it.permissions.isEmpty()}}}
        } finally {runBlocking {repository.delete(stored)}}
    }
    private fun capture(name:String) {
        compose.waitForIdle();android.os.SystemClock.sleep(250)
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val bitmap=instrumentation.uiAutomation.takeScreenshot()
        val file=java.io.File(compose.activity.getExternalFilesDir("screenshots"),"attachment.png")
        file.parentFile!!.mkdirs();file.outputStream().use {bitmap.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)};bitmap.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}

