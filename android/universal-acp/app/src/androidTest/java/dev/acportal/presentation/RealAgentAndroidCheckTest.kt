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
import kotlinx.coroutines.launch
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.serialization.json.*
import dev.acportal.protocol.*
import java.io.File
import java.util.concurrent.atomic.AtomicInteger

/**
 * Real-agent Android check for runbook §6: own-tools warning, host write notices,
 * approval (auto-review) badges, permission cards and forms, rendered by the production
 * conversation screen from a live session on a paired host.
 *
 * Off by default. It runs only with `-e acportalRealAgent 1` plus `realHostId` and
 * `realSessionId` (an existing session on a host already paired in the app). A model
 * call happens only if `realPrompt` is given, and then exactly once. The test never
 * answers a permission or submits a form in the observe-only method: those callbacks
 * fail the test. The separate submit method requires an independently human-authorized
 * acceptance fixture, validates its exact schema and consumes a single-use marker.
 */
class RealAgentAndroidCheckTest {
    @get:Rule val compose=createComposeRule()
    private val args=InstrumentationRegistry.getArguments()

    /** One separately human-authorized fixture submission, through the production screen. */
    @Test fun submitTheAuthorizedOwnedGooseForm()=runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        require(context.packageName=="dev.acportal.acceptance")
        require(args.getString("authorizeOwnedForm")=="true")
        val marker=File(context.cacheDir,"authorized-goose-form-consumed")
        check(!marker.exists()) {"This form authorization was already consumed"}
        val repository=(context.applicationContext as PortalApplication).repository
        val stored=repository.sessions.first().single {it.hostId=="real-goose-fixture" && it.draft=="Owned real draft 1"}
        val live=repository.open(stored.hostId,stored.id)
        val answers=AtomicInteger()
        val decisions=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        try {
            compose.waitUntil(30_000) {live.connection.value is dev.acportal.transport.ConnectionState.Connected && !live.state.value.replaying && live.state.value.permissions.isNotEmpty()}
            val pending=live.state.value.permissions.values.single()
            require(pending.isElicitation)
            val schema=pending.request["requestedSchema"]!!.jsonObject
            require(schema["properties"]!!.jsonObject.keys==setOf("colour","count"))
            require(schema["properties"]!!.jsonObject["colour"]!!.jsonObject["enum"]==JsonArray(listOf("red","green","blue").map(::JsonPrimitive)))
            require(schema["properties"]!!.jsonObject["count"]!!.jsonObject["minimum"]==JsonPrimitive(1))
            require(schema["properties"]!!.jsonObject["count"]!!.jsonObject["maximum"]==JsonPrimitive(5))
            compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
                SessionScreen(live,stored,onBack={},onPrompt={_,_->fail("No prompt submission")},onCancel={},
                    onPermission={_,_->fail("No additional tool approval")},onConfigure={_,_->},onDraft={},
                    onElicitation={permission,action,content->
                        assertEquals(pending,permission)
                        assertEquals("accept",action)
                        assertEquals(buildJsonObject {put("colour","green");put("count",2)},content)
                        check(marker.createNewFile()) {"Duplicate submission"}
                        assertEquals(1,answers.incrementAndGet())
                        decisions.launch {repository.elicitation(live,permission,action,content)}
                    })
            }}}
            compose.onNodeWithTag("elicitation-form").assertExists()
            compose.onNodeWithText("green").performScrollTo().performClick()
            compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("elicitation-field:count"))).performScrollTo().performTextInput("2")
            compose.onNodeWithTag("elicitation-submit").performScrollTo().performClick()
            compose.waitUntil(30_000) {live.state.value.permissions.isEmpty() && !live.state.value.processing}
            assertEquals(1,answers.get())
            assertEquals("Owned real draft 1",repository.sessions.first().single {it.hostId==stored.hostId && it.id==stored.id}.draft)
        } finally {decisions.cancel();live.close()}
        Unit
    }

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
