package dev.acportal.presentation

import dev.acportal.protocol.text
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import org.junit.Assert.assertEquals
import org.junit.Test

class SessionOptionSectionsTest {
    private fun options(json:String):List<JsonObject> = Json.parseToJsonElement(json).jsonArray.map {it.jsonObject}
    @Test fun modelAndWhatTunesItComeFirstThenModeThenTheRestInAgentOrder() {
        // In the agent's order: an uncategorised provider, the mode, effort, the model, context size.
        val list=options("""[{"id":"provider","type":"select"},{"id":"mode","category":"mode"},{"id":"effort","category":"thought_level"},{"id":"m","category":"model"},{"id":"ctx","category":"model_config"},{"id":"other","category":"_custom"}]""")
        val sections=optionSections(list)
        assertEquals(listOf("Model","Mode","More options"),sections.map {it.title})
        assertEquals(listOf(listOf("m","effort","ctx"),listOf("mode"),listOf("provider","other")),sections.map {s->s.options.map {it["id"].text()}})
    }
    @Test fun emptySectionsAreLeftOutAndAnUncategorisedModelIdCountsAsTheModel() {
        val sections=optionSections(options("""[{"id":"model"},{"id":"speed","category":"model_config"}]"""))
        assertEquals(listOf("Model"),sections.map {it.title})
        assertEquals(listOf("model","speed"),sections.single().options.map {it["id"].text()})
        assertEquals(emptyList<OptionSection>(),optionSections(emptyList()))
    }
}
