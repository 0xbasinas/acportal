package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.InstalledBuild
import dev.acportal.data.ReleaseInfo
import dev.acportal.data.UpdateState
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test

class UpdateScreenUiTest {
    @get:Rule val compose=createComposeRule()

    private val installed=InstalledBuild("0.1.0-debug.12+3ec98db",12)
    private val release=ReleaseInfo("0.1.0-debug.15+df33193",15,"acportal-debug-15-df33193.apk","https://github.com/x.apk",54L*1024*1024,"a".repeat(64),"2026-10-11T15:02:00Z")
    private class Calls {var check=0;var install:ReleaseInfo?=null;var cancel=0;var settings=0}

    private fun show(theme:String,state:UpdateState,calls:Calls) {
        compose.setContent {PortalTheme(theme) {UpdateScreen(state,installed,{},{calls.check++},{calls.install=it},{calls.cancel++},{calls.settings++})}}
    }

    @Test fun availableShowsTheBuildAndInstallsOnlyWhenAsked() {
        val calls=Calls()
        show("dark",UpdateState.Available(release),calls)
        compose.onNodeWithText("0.1.0-debug.12+3ec98db").assertIsDisplayed()
        compose.onNodeWithText("Update available").assertIsDisplayed()
        compose.onNodeWithText("0.1.0-debug.15+df33193").assertIsDisplayed()
        compose.onNodeWithText("Build 15 · 54 MB",substring=true).assertIsDisplayed()
        assertNull("nothing happens before the button is pressed",calls.install)
        compose.onNodeWithTag("update-primary").performClick()
        assertEquals(release,calls.install)
        compose.onNodeWithTag("update-secondary").performClick()
        assertEquals(1,calls.check)
    }
    @Test fun lightAvailableIsReadable() {
        show("light",UpdateState.Available(release.copy(sha256=null)),Calls())
        compose.onNodeWithText("This release has no checksum",substring=true).assertIsDisplayed()
        compose.onNodeWithText("Download and install").assertIsDisplayed()
    }
    @Test fun upToDateOnlyOffersACheck() {
        val calls=Calls()
        show("dark",UpdateState.UpToDate(release),calls)
        compose.onNodeWithText("You're up to date").assertIsDisplayed()
        compose.onNodeWithTag("update-primary").assertDoesNotExist()
        compose.onNodeWithTag("update-secondary").performClick()
        assertEquals(1,calls.check)
    }
    @Test fun downloadingShowsProgressAndCanBeCancelled() {
        val calls=Calls()
        show("light",UpdateState.Downloading(release,22L*1024*1024),calls)
        compose.onNodeWithText("Downloading build 15").assertIsDisplayed()
        compose.onNodeWithText("22 MB of 54 MB").assertIsDisplayed()
        compose.onNodeWithTag("update-progress").assertExists()
        compose.onNodeWithText("Cancel").performClick()
        assertEquals(1,calls.cancel)
    }
    @Test fun permissionStateOpensSystemSettings() {
        val calls=Calls()
        show("dark",UpdateState.NeedsPermission(release),calls)
        compose.onNodeWithText("Allow installs from this app").assertIsDisplayed()
        compose.onNodeWithText("Open settings").performClick()
        assertEquals(1,calls.settings)
    }
    @Test fun failedCheckRetriesTheCheckAndFailedInstallRetriesThatBuild() {
        val check=Calls()
        show("light",UpdateState.Failed("GitHub could not be reached."),check)
        compose.onNodeWithText("Could not check for updates").assertIsDisplayed()
        compose.onNodeWithText("Try again").performClick()
        assertEquals(1,check.check);assertNull(check.install)
    }
    @Test fun failedInstallKeepsTheBuildForRetry() {
        val calls=Calls()
        show("dark",UpdateState.Failed("This build is signed with a different key.",release),calls)
        compose.onNodeWithText("The update did not finish").assertIsDisplayed()
        compose.onNodeWithText("Try again").performClick()
        assertEquals(release,calls.install);assertEquals(0,calls.check)
    }
    @Test fun checkingAndInstallingOfferNoButtons() {
        show("dark",UpdateState.Checking,Calls())
        compose.onNodeWithText("Checking for updates…").assertIsDisplayed()
        compose.onNodeWithTag("update-primary").assertDoesNotExist()
        compose.onNodeWithTag("update-secondary").assertDoesNotExist()
    }
}
