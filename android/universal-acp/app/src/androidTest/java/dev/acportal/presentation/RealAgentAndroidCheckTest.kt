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
import java.net.HttpURLConnection
import java.net.URL

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

    private fun hostOption(permission:Permission,expectedPath:String,expectedContent:String):JsonObject {
        fun normalizedPath(value:JsonElement)=value.jsonPrimitive.content.replace('\\','/').removePrefix("//?/")
        val request=permission.request
        require(!permission.isElicitation && request["_meta"]?.jsonObject?.get("acpdSource")==JsonPrimitive("host-filesystem"))
        val tool=request.getValue("toolCall").jsonObject
        val raw=tool.getValue("rawInput").jsonObject
        require(raw.keys==setOf("path") && normalizedPath(raw.getValue("path")).equals(expectedPath,ignoreCase=true)) {"Host consent path differs from the owned target"}
        val diff=tool.getValue("content").jsonArray.single().jsonObject
        require(diff["type"]==JsonPrimitive("diff") && diff["newText"]==JsonPrimitive(expectedContent))
        require(diff["oldText"]==JsonNull) {"A new owned file must have no prior contents"}
        require(normalizedPath(diff.getValue("path")).equals(expectedPath,ignoreCase=true))
        return request.getValue("options").jsonArray.single {it.jsonObject["optionId"]==JsonPrimitive("acpd-write-allow") && it.jsonObject["kind"]==JsonPrimitive("allow_once")}.jsonObject
    }

    @Test fun ownedWriteConsentRejectsDifferentFilesContentsAndPriorData() {
        val target="C:/owned/reload-fixture.txt"
        fun permission(path:String="\\\\?\\C:\\owned\\reload-fixture.txt",content:String="fixture",old:JsonElement=JsonNull)=Permission(JsonPrimitive("owned-host"),buildJsonObject {
            putJsonObject("_meta") {put("acpdSource","host-filesystem")}
            putJsonObject("toolCall") {
                putJsonObject("rawInput") {put("path",path)}
                putJsonArray("content") {add(buildJsonObject {put("type","diff");put("path",path);put("oldText",old);put("newText",content)})}
            }
            putJsonArray("options") {add(buildJsonObject {put("optionId","acpd-write-allow");put("kind","allow_once")})}
        })
        assertEquals("acpd-write-allow",hostOption(permission(),target,"fixture")["optionId"]!!.jsonPrimitive.content)
        for(invalid in listOf(permission(path="C:/other/reload-fixture.txt"),permission(content="fixture\n"),permission(old=JsonPrimitive("existing")),permission(path="C:/owned/../other/reload-fixture.txt"))) {
            assertTrue("Unmatched consent must be refused",runCatching {hostOption(invalid,target,"fixture")}.exceptionOrNull() is IllegalArgumentException)
        }
    }

    @Test fun approveTheAuthorizedOwnedGooseWrite()=runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        require(context.packageName=="dev.acportal.acceptance" && args.getString("authorizeOwnedWrite")=="true")
        val port=requireNotNull(args.getString("realBootstrapPort")?.toIntOrNull())
        require(port in 1024..65535)
        val bootstrap=URL("http://127.0.0.1:$port/bootstrap").openConnection().apply {connectTimeout=5000;readTimeout=5000}
            .getInputStream().use {WireJson.parseToJsonElement(it.readBytes().toString(Charsets.UTF_8)).jsonObject}
        val expected=bootstrap.getValue("writePermission").jsonObject
        val expectedContent=bootstrap.getValue("expectedWriteContent").jsonPrimitive.content
        val expectNotice=bootstrap.getValue("expectHostWriteNotice").jsonPrimitive.boolean
        require(expectedContent=="fixture")
        val expectedPath=bootstrap.getValue("expectedWritePath").jsonPrimitive.content.replace('\\','/')
        require(expectedPath.endsWith("/reload-fixture.txt"))
        val repository=(context.applicationContext as PortalApplication).repository
        val stored=repository.sessions.first().single {it.hostId=="real-goose-fixture" && it.draft=="Owned real draft 1"}
        val live=repository.open(stored.hostId,stored.id)
        val decisions=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val count=AtomicInteger()
        try {
            compose.waitUntil(30_000) {live.connection.value is dev.acportal.transport.ConnectionState.Connected && !live.state.value.replaying && live.state.value.permissions.isNotEmpty()}
            val pending=live.state.value.permissions.values.single()
            require(!pending.isElicitation && pending.id==expected["id"] && pending.request==expected["params"])
            val raw=pending.request.getValue("toolCall").jsonObject.getValue("rawInput").jsonObject
            require(raw.keys==setOf("path","content") && raw["content"]==JsonPrimitive(expectedContent))
            val path=raw.getValue("path").jsonPrimitive.content.replace('\\','/')
            require(path=="reload-fixture.txt" || path.equals(expectedPath,ignoreCase=true))
            require(live.info.ownTools)
            val option=pending.request.getValue("options").jsonArray.single {it.jsonObject["kind"]==JsonPrimitive("allow_once")}.jsonObject
            compose.setContent {PortalTheme {Surface(Modifier.fillMaxSize()) {
                SessionScreen(live,stored,onBack={},onPrompt={_,_->fail("No additional prompt")},onCancel={},
                    onPermission={permission,id->
                        val endpoint=if(permission.id==pending.id) {
                            assertEquals(pending,permission);assertEquals(option.getValue("optionId").jsonPrimitive.content,id)
                            assertEquals("Duplicate agent approval",1,count.incrementAndGet())
                            "consume-write"
                        } else {
                            assertEquals(hostOption(permission,expectedPath,expectedContent).getValue("optionId").jsonPrimitive.content,id)
                            assertEquals("Duplicate host consent",2,count.incrementAndGet())
                            "consume-host-write"
                        }
                        decisions.launch {
                            val consume=URL("http://127.0.0.1:$port/$endpoint").openConnection() as HttpURLConnection
                            try {
                                consume.connectTimeout=5000;consume.readTimeout=5000;consume.requestMethod="POST";consume.doOutput=true;consume.setFixedLengthStreamingMode(0)
                                consume.outputStream.use {}
                                check(consume.responseCode==204) {"Owned write authorization unavailable"}
                                repository.permission(live,permission,id)
                            } finally {consume.disconnect()}
                        }
                    },onConfigure={_,_->},onDraft={},onElicitation={_,_,_->fail("No form decision")})
            }}}
            compose.onNodeWithText(option.getValue("name").jsonPrimitive.content).performScrollTo().performClick()
            compose.waitUntil(60_000) {!live.state.value.processing || live.state.value.permissions.values.any {it.id!=pending.id}}
            live.state.value.permissions.values.singleOrNull {it.id!=pending.id}?.let {host->
                val hostChoice=hostOption(host,expectedPath,expectedContent)
                compose.onNodeWithText(hostChoice.getValue("name").jsonPrimitive.content).performScrollTo().performClick()
            }
            compose.waitUntil(60_000) {!live.state.value.processing && live.state.value.permissions.isEmpty()}
            assertTrue(count.get() in 1..2)
            compose.onNodeWithTag("own-tools-warning",useUnmergedTree=true).assertIsDisplayed()
            if(expectNotice) {
                assertTrue("Real write did not produce a host notice",live.state.value.items.any {it is TimelineItem.Tool && it.hostWrite!=null})
                compose.onAllNodesWithTag("host-write-badge",useUnmergedTree=true).onFirst().assertExists()
            }
            assertEquals("Owned real draft 1",repository.sessions.first().single {it.hostId==stored.hostId && it.id==stored.id}.draft)
        } finally {decisions.cancel();live.close()}
        Unit
    }

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
