package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Surface
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import java.util.concurrent.atomic.AtomicBoolean

/** Isolated rendered form: no repository, vault, network or persisted user input. */
class ElicitationScreenUiTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)
    @org.junit.After fun closeKeyboard() {androidx.test.espresso.Espresso.closeSoftKeyboard()}
    @Test fun darkFormValidatesAndRequiresExplicitAnswer()=checkForm("dark")
    @Test fun lightFormValidatesAndRequiresExplicitAnswer()=checkForm("light")
    @Test fun darkBooleanAndArraySelectionsUseTypedAnswers()=checkForm("dark",true)
    @Test fun lightBooleanAndArraySelectionsUseTypedAnswers()=checkForm("light",true)

    private val permission=Permission(JsonPrimitive("fixture-form"),WireJson.parseToJsonElement("""{
        "mode":"form","message":"Fixture information request","requestedSchema":{"type":"object","properties":{
        "name":{"type":"string","title":"Name"},"age":{"type":"integer","title":"Age","minimum":0},
        "confidence":{"type":"number","title":"Confidence","minimum":0,"maximum":1},
        "choice":{"type":"string","title":"Priority","oneOf":[{"const":" padded ","title":"Padded choice"}]},
        "confirmed":{"type":"boolean","title":"Confirmed"},
        "tags":{"type":"array","title":"Tags","minItems":0,"items":{"type":"string","enum":["a","b"]}}
        },"required":["name","age","confidence","choice","confirmed","tags"]}}
        """.trimIndent()).jsonObject,ELICITATION_METHOD)

    @OptIn(ExperimentalLayoutApi::class)
    private fun checkForm(mode:String,selectFlags:Boolean=false) {
        if(InstrumentationRegistry.getArguments().getString("requireLargeFont")=="true")
            assertTrue(InstrumentationRegistry.getInstrumentation().targetContext.resources.configuration.fontScale>=1.9f)
        val keyboard=AtomicBoolean(false)
        val answers=mutableListOf<Pair<String,JsonObject?>>()
        var enabled by mutableStateOf(true)
        compose.setContent {
            val visible=WindowInsets.isImeVisible
            SideEffect {keyboard.set(visible)}
            PortalTheme(mode) {Surface(Modifier.fillMaxSize().systemBarsPadding().imePadding()) {
                Column(Modifier.verticalScroll(rememberScrollState())) {ElicitationRequest(permission,enabled) {action,content->answers.add(action to content)}}
            }}
        }
        compose.onNodeWithTag("elicitation-submit").performScrollTo().performTouchInput {click()}
        compose.runOnIdle {assertTrue(answers.isEmpty())}
        compose.onNodeWithText("Fix the highlighted fields to submit.").assertExists()
        fun input(name:String,value:String) {
            val field=compose.onNode(hasSetTextAction() and hasAnyAncestor(hasTestTag("elicitation-field:$name")))
            field.performScrollTo().performTouchInput {click()}
            field.performTextInput(value)
            compose.waitUntil(10_000) {keyboard.get()}
        }
        input("name","Fixture answer")
        input("age","21")
        input("confidence","0.5")
        compose.onNodeWithText("Padded choice").performScrollTo().performTouchInput {click()}
        if(selectFlags) {
            compose.onNode(hasText("Confirmed",substring=true) and isToggleable()).performScrollTo().performTouchInput {click()}
            compose.onNode(hasText("a") and isToggleable()).performScrollTo().performTouchInput {click()}
        }
        compose.runOnIdle {assertTrue(answers.isEmpty())}
        compose.onNodeWithTag("elicitation-submit").performScrollTo().assertIsDisplayed()
        assertTrue("Native keyboard remains visible at Submit",keyboard.get())
        compose.onNodeWithTag("elicitation-submit").performTouchInput {click()}
        compose.runOnIdle {
            assertEquals(1,answers.size);assertEquals("accept",answers.single().first)
            val content=answers.single().second!!
            assertEquals(JsonPrimitive(" padded "),content["choice"])
            assertEquals(JsonArray(if(selectFlags)listOf(JsonPrimitive("a")) else emptyList()),content["tags"])
            assertEquals(JsonPrimitive(21L),content["age"])
            assertEquals(JsonPrimitive(0.5),content["confidence"])
            assertEquals(JsonPrimitive(selectFlags),content["confirmed"])
            enabled=false
        }
        listOf("elicitation-submit","elicitation-decline","elicitation-cancel").forEach {compose.onNodeWithTag(it).assertIsNotEnabled()}
        compose.runOnIdle {enabled=true}
        compose.onNodeWithTag("elicitation-decline").performScrollTo().performTouchInput {click()}
        compose.onNodeWithTag("elicitation-cancel").performScrollTo().performTouchInput {click()}
        compose.runOnIdle {assertEquals(listOf("accept","decline","cancel"),answers.map {it.first});assertNull(answers[1].second);assertNull(answers[2].second)}
    }
}
