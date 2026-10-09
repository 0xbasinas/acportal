package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ElicitationValueTest {
    private fun form(schema: String) = elicitationForm(Permission(JsonPrimitive("review"),
        WireJson.parseToJsonElement("""{"mode":"form","message":"Choose","requestedSchema":$schema}""").jsonObject, ELICITATION_METHOD))

    @Test fun advertisedChoiceValueIsPreservedExactly() {
        val form = form("""{"type":"object","properties":{"choice":{"type":"string","enum":[" padded "]}},"required":["choice"]}""")
        val input = ElicitationInput(text = mapOf("choice" to " padded "))
        assertTrue("The advertised choice must be selectable", form.problems(input).isEmpty())
        assertEquals(JsonPrimitive(" padded "), form.content(input)["choice"])
    }

    @Test fun requiredArrayCanBeEmptyWhenMinItemsIsZero() {
        val form = form("""{"type":"object","properties":{"tags":{"type":"array","minItems":0,"items":{"type":"string","enum":["a"]}}},"required":["tags"]}""")
        val input = ElicitationInput(selections = mapOf("tags" to emptySet()))
        assertTrue("Required names presence, not a minimum item count", form.problems(input).isEmpty())
        assertEquals(JsonArray(emptyList()), form.content(input)["tags"])
    }

    @Test fun emptyChoiceIsDifferentFromNoChoice() {
        val form = form("""{"type":"object","properties":{"choice":{"type":"string","enum":[""]}},"required":["choice"]}""")
        assertEquals("Required", form.problems(ElicitationInput())["choice"])
        val input = ElicitationInput(text = mapOf("choice" to ""))
        assertTrue(form.problems(input).isEmpty())
        assertEquals(JsonPrimitive(""), form.content(input)["choice"])
    }

    @Test fun requiredArrayStillHonorsItsItemBounds() {
        val form = form("""{"type":"object","properties":{"tags":{"type":"array","minItems":1,"maxItems":1,"items":{"type":"string","enum":["a","b"]}}},"required":["tags"]}""")
        assertEquals("Choose at least 1", form.problems(ElicitationInput())["tags"])
        assertEquals("Choose at most 1", form.problems(ElicitationInput(selections = mapOf("tags" to setOf("a", "b"))))["tags"])
        assertTrue(form.problems(ElicitationInput(selections = mapOf("tags" to setOf("a")))).isEmpty())
    }

    @Test fun optionalUnselectedFieldsStayOmitted() {
        val form = form("""{"type":"object","properties":{"choice":{"type":"string","enum":[" padded "]},"tags":{"type":"array","minItems":1,"items":{"type":"string","enum":["a"]}}}}""")
        assertTrue(form.problems(ElicitationInput()).isEmpty())
        assertEquals(JsonObject(emptyMap()), form.content(ElicitationInput()))
        assertEquals("Choose one of the options", form.problems(ElicitationInput(text = mapOf("choice" to "padded")))["choice"])
    }
}
