package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material3.Surface
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test

/**
 * Real-agent Android check for runbook §6: own-tools warning, host write notices,
 * approval (auto-review) badges, permission cards and forms, rendered by the production
 * conversation screen from a live session on a paired host.
 *
 * Off by default. It runs only with `-e acportalRealAgent 1` plus `realHostId` and
 * `realSessionId` (an existing session on a host already paired in the app). A model
 * call happens only if `realPrompt` is given, and then exactly once. The test never
 * answers a permission or submits a form: those callbacks fail the test. At the end it
 * cancels a still-running turn.
 */
class RealAgentAndroidCheckTest {
    @get:Rule val compose=createComposeRule()
    private val args=InstrumentationRegistry.getArguments()

    @Test fun liveSessionShowsTheExpectedNoticesWithoutAnswering()=runBlocking {
        assumeTrue("Real-agent checks need -e acportalRealAgent 1 (each run is an explicit go-ahead)",args.getString("acportalRealAgent")=="1")
        val hostId=args.getString("realHostId");val sessionId=args.getString("realSessionId")
        assumeTrue("Set -e realHostId and -e realSessionId",!hostId.isNullOrBlank() && !sessionId.isNullOrBlank())
        val expected=args.getString("expect","ownTools,hostWrite,autoReview").split(',').map {it.trim()}.filter {it.isNotEmpty()}.toSet()
        val unknown=expected-setOf("ownTools","hostWrite","autoReview","permission","form")
        assertTrue("Unknown expectations $unknown",unknown.isEmpty())
        val timeout=(args.getString("realTimeoutSeconds")?.toLongOrNull() ?: 180L)*1000
        val repository=(InstrumentationRegistry.getInstrumentation().targetContext.applicationContext as PortalApplication).repository
        val stored=repository.sessions.first().singleOrNull {it.hostId==hostId && it.id==sessionId}
        assertNotNull("Open the session in the app once so it is saved on this device",stored)
        val live=repository.open(hostId!!,sessionId!!)
        try {
            compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
                SessionScreen(live,stored!!,onBack={},onPrompt={_,_->fail("The check never sends from the composer")},onCancel={},
                    onPermission={_,_->fail("The check never answers a permission")},onConfigure={_,_->},onDraft={},
                    onElicitation={_,_,_->fail("The check never submits a form")})
            }}}
            compose.waitUntil(30_000) {live.connection.value is dev.acportal.transport.ConnectionState.Connected && !live.state.value.replaying}
            args.getString("realPrompt")?.takeIf {it.isNotBlank()}?.let {repository.prompt(live,it)}
            fun seen(tag:String)=compose.onAllNodesWithTag(tag,useUnmergedTree=true).fetchSemanticsNodes().isNotEmpty()
            val checks=mapOf<String,()->Boolean>(
                "ownTools" to {seen("own-tools-warning")},
                "hostWrite" to {live.state.value.items.any {it is dev.acportal.protocol.TimelineItem.Tool && it.hostWrite!=null}},
                "autoReview" to {live.state.value.items.any {it is dev.acportal.protocol.TimelineItem.Tool && it.autoReview!=null}},
                "permission" to {live.state.value.permissions.isNotEmpty()},
                "form" to {seen("elicitation-form")},
            )
            compose.waitUntil(timeout) {expected.all {checks.getValue(it)()}}
            if("hostWrite" in expected)compose.onAllNodesWithTag("host-write-badge",useUnmergedTree=true).onFirst().assertExists()
            if("autoReview" in expected)compose.onAllNodesWithTag("auto-review-badge",useUnmergedTree=true).onFirst().assertExists()
        } finally {
            if(live.state.value.processing)runCatching {repository.cancel(live)}
            live.close()
        }
        Unit
    }
}
