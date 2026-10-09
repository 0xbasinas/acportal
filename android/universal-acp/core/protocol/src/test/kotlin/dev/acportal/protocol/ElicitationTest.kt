package dev.acportal.protocol

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class ElicitationTest {
    private fun event(sequence:Long,message:String,direction:String="agent")=WireJson.parseToJsonElement("""{"type":"event","sequence":$sequence,"direction":"$direction","message":$message}""").objectValue()
    /** The schema testy sends (ACP rust-sdk testy_elicitation_schema). */
    private val testySchema="""{"type":"object","title":"Testy elicitation form","description":"Deterministic form","properties":{
        "name":{"type":"string"},"email":{"type":"string","format":"email"},"homepage":{"type":"string","format":"uri"},
        "birthday":{"type":"string","format":"date"},"available_at":{"type":"string","format":"date-time"},
        "confidence":{"type":"number","minimum":0.0,"maximum":1.0},"age":{"type":"integer","minimum":0,"maximum":120},
        "confirmed":{"type":"boolean"},"priority":{"type":"string","enum":["low","normal","high"]},
        "tags":{"type":"array","items":{"type":"string","enum":["rust","acp","testy"]}}},
        "required":["name","confidence","age","confirmed"]}"""
    private fun request(id:String="e1",schema:String=testySchema)=event(1,"""{"jsonrpc":"2.0","id":"$id","method":"elicitation/create","params":{"mode":"form","sessionId":"s","toolCallId":"t","message":"Accept the Testy form","requestedSchema":$schema}}""")

    @Test fun formElicitationsBecomePendingQuestionsAndLeaveWithTheAnswer() {
        val state=SessionReducer.reduce(SessionState(),request())
        val pending=state.permissions.values.single()
        assertTrue(pending.isElicitation);assertEquals("Accept the Testy form",pending.title)
        // The client's answer clears it, like a permission response.
        val answered=SessionReducer.reduce(state,event(2,"""{"jsonrpc":"2.0","id":"e1","result":{"action":"decline"}}""","client"))
        assertTrue(answered.permissions.isEmpty())
        // Replayed pending questions keep their kind.
        val replay=SessionReducer.reduce(SessionReducer.reduce(SessionState(),WireJson.parseToJsonElement("""{"type":"replay_start"}""").objectValue()),
            WireJson.parseToJsonElement("""{"type":"pending_permission","message":{"jsonrpc":"2.0","id":"e1","method":"elicitation/create","params":{"mode":"form","sessionId":"s","message":"Again","requestedSchema":$testySchema}}}""").objectValue())
        val restored=SessionReducer.reduce(replay,WireJson.parseToJsonElement("""{"type":"replay_complete","latestSequence":1}""").objectValue())
        assertTrue(restored.permissions.values.single().isElicitation)
        // URL mode and malformed requests are not shown; ordinary permissions are unchanged.
        val url=SessionReducer.reduce(SessionState(),event(1,"""{"jsonrpc":"2.0","id":"u","method":"elicitation/create","params":{"mode":"url","sessionId":"s","elicitationId":"x","url":"https://example.com","message":"Open"}}"""))
        assertTrue(url.permissions.isEmpty())
        val permission=SessionReducer.reduce(SessionState(),event(1,"""{"jsonrpc":"2.0","id":"p","method":"session/request_permission","params":{"sessionId":"s","toolCall":{"toolCallId":"t","title":"Run"},"options":[{"optionId":"a","name":"Allow","kind":"allow_once"}]}}"""))
        assertFalse(permission.permissions.values.single().isElicitation)
        // Older saved permissions decode as permissions.
        assertFalse(WireJson.decodeFromString<Permission>("""{"id":1,"request":{}}""").isElicitation)
    }

    @Test fun testySchemaBuildsEveryFieldKindAndValidates() {
        val form=elicitationForm(SessionReducer.reduce(SessionState(),request()).permissions.values.single())
        assertTrue(form.canAccept);assertEquals("Testy elicitation form",form.title)
        val kinds=form.fields.associate {it.name to it.kind}
        assertEquals(ElicitationKind.TEXT,kinds["name"]);assertEquals(ElicitationKind.NUMBER,kinds["confidence"]);assertEquals(ElicitationKind.INTEGER,kinds["age"])
        assertEquals(ElicitationKind.BOOLEAN,kinds["confirmed"]);assertEquals(ElicitationKind.CHOICE,kinds["priority"]);assertEquals(ElicitationKind.MULTI_CHOICE,kinds["tags"])
        assertEquals("date-time",form.fields.first {it.name=="available_at"}.format)
        val empty=form.initialInput()
        assertEquals(setOf("name","confidence","age"),form.problems(empty).keys)
        val filled=empty.copy(text=mapOf("name" to " Ada ","confidence" to "0.5","age" to "36","priority" to "high","email" to "ada@example.com","birthday" to "1815-12-10"),
            flags=mapOf("confirmed" to true),selections=mapOf("tags" to setOf("testy","rust")))
        assertEquals(emptyMap<String,String>(),form.problems(filled))
        val content=form.content(filled)
        assertEquals(JsonPrimitive("Ada"),content["name"]);assertEquals(JsonPrimitive(0.5),content["confidence"]);assertEquals(JsonPrimitive(36L),content["age"])
        assertEquals(JsonPrimitive(true),content["confirmed"]);assertEquals(JsonPrimitive("high"),content["priority"])
        assertEquals(listOf("rust","testy"),content["tags"]!!.jsonArray.map {it.jsonPrimitive.content})
        assertNull(content["homepage"])
        val bad=filled.copy(text=filled.text+mapOf("confidence" to "2","age" to "3.5","email" to "nope","homepage" to "example.com","available_at" to "tomorrow","priority" to "urgent"))
        assertEquals(setOf("confidence","age","email","homepage","available_at","priority"),form.problems(bad).keys)
        assertEquals("Use 1 or less",form.problems(bad)["confidence"])
    }

    @Test fun answersAreWellFormedAndUnsupportedRequiredFieldsBlockAccept() {
        val pending=SessionReducer.reduce(SessionState(),request()).permissions.values.single()
        val accept=elicitationResponse(pending,"accept",buildJsonObject {put("name","Ada")})
        assertEquals("""{"jsonrpc":"2.0","id":"e1","result":{"action":"accept","content":{"name":"Ada"}}}""",accept.toString())
        assertEquals("""{"jsonrpc":"2.0","id":"e1","result":{"action":"decline"}}""",elicitationResponse(pending,"decline").toString())
        assertEquals("""{"jsonrpc":"2.0","id":"e1","result":{"action":"cancel"}}""",elicitationResponse(pending,"cancel").toString())
        assertThrows(IllegalArgumentException::class.java) {elicitationResponse(pending,"open")}
        assertThrows(IllegalArgumentException::class.java) {elicitationResponse(Permission(JsonPrimitive(1),JsonObject(emptyMap())),"cancel")}
        val odd=elicitationForm(SessionReducer.reduce(SessionState(),request("e2","""{"type":"object","properties":{"blob":{"type":"object"},"note":{"type":"string","default":"hi","maxLength":3}},"required":["blob","missing"]}""")).permissions.values.single())
        assertFalse(odd.canAccept);assertEquals(listOf("blob","missing"),odd.unsupportedRequired)
        assertEquals("hi",odd.initialInput().text["note"])
        assertEquals("Use at most 3 characters",odd.problems(ElicitationInput(text=mapOf("note" to "long")))["note"])
        // oneOf titles label single choices; anyOf titles label multi choices.
        val titled=elicitationForm(SessionReducer.reduce(SessionState(),request("e3","""{"type":"object","properties":{"size":{"type":"string","oneOf":[{"const":"s","title":"Small"},{"const":"l","title":"Large"}],"default":"l"},"days":{"type":"array","minItems":1,"maxItems":2,"items":{"anyOf":[{"const":"mo","title":"Monday"},{"const":"tu","title":"Tuesday"},{"const":"we","title":"Wednesday"}]}}}}""")).permissions.values.single())
        assertEquals(listOf("Small","Large"),titled.fields.first {it.name=="size"}.choices.map {it.label})
        assertEquals("l",titled.initialInput().text["size"])
        assertEquals("Choose at most 2",titled.problems(ElicitationInput(selections=mapOf("days" to setOf("mo","tu","we"))))["days"])
        assertNull(titled.content(ElicitationInput())["days"])
    }

    @Test fun limitsAndEdgeValues() {
        // Only the first 64 fields are shown; a required field past the limit blocks Accept.
        val many=(0 until 65).joinToString(",") {"\"f$it\":{\"type\":\"string\"}"}
        val capped=elicitationForm(SessionReducer.reduce(SessionState(),request("e4","""{"type":"object","properties":{$many},"required":["f64"]}""")).permissions.values.single())
        assertEquals(64,capped.fields.size);assertFalse(capped.canAccept);assertEquals(listOf("f64"),capped.unsupportedRequired)
        // Negative bounds, whitespace-only text and minLength.
        val edges=elicitationForm(SessionReducer.reduce(SessionState(),request("e5","""{"type":"object","properties":{"n":{"type":"integer","minimum":-5},"x":{"type":"number"},"code":{"type":"string","minLength":2}},"required":["code"]}""")).permissions.values.single())
        assertEquals(mapOf("n" to "Use -5 or more","x" to "Enter a number","code" to "Required"),edges.problems(ElicitationInput(text=mapOf("n" to "-6","x" to "NaN","code" to "   "))))
        assertEquals("Use at least 2 characters",edges.problems(ElicitationInput(text=mapOf("code" to "a")))["code"])
        assertEquals(emptyMap<String,String>(),edges.problems(ElicitationInput(text=mapOf("n" to "-5","x" to "1e3","code" to "ab"))))
        assertEquals(JsonPrimitive(-5L),edges.content(ElicitationInput(text=mapOf("n" to "-5","code" to "ab")))["n"])
        // Text longer than the global limit is refused even without maxLength.
        val text=elicitationForm(SessionReducer.reduce(SessionState(),request("e6","""{"type":"object","properties":{"t":{"type":"string"}}}""")).permissions.values.single())
        assertNotNull(text.problems(ElicitationInput(text=mapOf("t" to "a".repeat(10_001))))["t"])
        assertNull(text.problems(ElicitationInput(text=mapOf("t" to "a".repeat(10_000))))["t"])
    }
}
