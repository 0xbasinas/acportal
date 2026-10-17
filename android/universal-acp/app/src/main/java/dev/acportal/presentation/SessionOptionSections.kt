package dev.acportal.presentation

import dev.acportal.protocol.text
import kotlinx.serialization.json.JsonObject

/**
 * Session options shown together, as the ACP spec suggests: the model with what tunes it
 * (`model_config`, `thought_level`), then the mode, then the agent's other options. Within a
 * section the agent's order is kept, except that the model itself comes first. The desktop
 * groups them the same way (desktop/src/agentSetup.ts `sections`).
 */
internal data class OptionSection(val title:String,val options:List<JsonObject>)

internal fun optionSections(options:List<JsonObject>):List<OptionSection> {
    fun category(option:JsonObject)=option["category"].text()
    fun isModel(option:JsonObject)=category(option)=="model" || (category(option).isEmpty() && option["id"].text()=="model")
    val model=options.filter(::isModel)+options.filter {!isModel(it) && category(it) in setOf("model_config","thought_level")}
    val mode=options.filter {it !in model && category(it)=="mode"}
    val more=options.filter {it !in model && it !in mode}
    return listOf(OptionSection("Model",model),OptionSection("Mode",mode),OptionSection("More options",more)).filter {it.options.isNotEmpty()}
}
