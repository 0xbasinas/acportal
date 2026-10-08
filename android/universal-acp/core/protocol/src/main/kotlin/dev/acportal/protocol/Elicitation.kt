package dev.acportal.protocol

import kotlinx.serialization.json.*

/** Field kinds the phone can show for an ACP form elicitation (the stable restricted JSON Schema subset). */
enum class ElicitationKind { TEXT, NUMBER, INTEGER, BOOLEAN, CHOICE, MULTI_CHOICE }

data class ElicitationChoice(val value: String, val label: String)

data class ElicitationField(
    val name: String,
    val label: String,
    val description: String?,
    val kind: ElicitationKind,
    val required: Boolean,
    val choices: List<ElicitationChoice> = emptyList(),
    /** String format: email, uri, date or date-time. */
    val format: String? = null,
    val minimum: Double? = null,
    val maximum: Double? = null,
    val minLength: Int? = null,
    val maxLength: Int? = null,
    val minItems: Int? = null,
    val maxItems: Int? = null,
    val default: JsonElement? = null,
)

/** What the user has entered so far. Text, numbers and single choices are kept as text. */
data class ElicitationInput(
    val text: Map<String, String> = emptyMap(),
    val flags: Map<String, Boolean> = emptyMap(),
    val selections: Map<String, Set<String>> = emptyMap(),
)

data class ElicitationForm(
    val message: String,
    val title: String?,
    val description: String?,
    val fields: List<ElicitationField>,
    /** Required properties the phone cannot show; the form can then only be declined or cancelled. */
    val unsupportedRequired: List<String>,
) {
    val canAccept: Boolean get() = unsupportedRequired.isEmpty()
}

/** Bounds keep a hostile schema from building an unbounded form. */
const val ELICITATION_MAX_FIELDS = 64
const val ELICITATION_MAX_CHOICES = 200
const val ELICITATION_MAX_TEXT = 10_000

private fun JsonElement?.doubleOrNull(): Double? = (this as? JsonPrimitive)?.takeUnless { it.isString }?.doubleOrNull
private fun JsonElement?.intOrNull(): Int? = (this as? JsonPrimitive)?.takeUnless { it.isString }?.intOrNull

fun elicitationForm(permission: Permission): ElicitationForm {
    val request = permission.request
    val schema = request["requestedSchema"].objectValue()
    val required = schema["required"].arrayValue().map { it.text() }.toSet()
    val fields = ArrayList<ElicitationField>()
    val unsupported = ArrayList<String>()
    for ((name, value) in schema["properties"].objectValue().entries.take(ELICITATION_MAX_FIELDS)) {
        val property = value.objectValue()
        val field = elicitationField(name, property, name in required)
        if (field != null) fields.add(field) else if (name in required) unsupported.add(name)
    }
    // Required names the schema never describes cannot be filled in either.
    unsupported.addAll(required.filter { name -> name.isNotEmpty() && fields.none { it.name == name } && name !in unsupported })
    return ElicitationForm(request["message"].text().take(4000), schema["title"].text().takeIf { it.isNotBlank() }?.take(200),
        schema["description"].text().takeIf { it.isNotBlank() }?.take(2000), fields, unsupported)
}

private fun elicitationField(name: String, property: JsonObject, required: Boolean): ElicitationField? {
    val label = property["title"].text().ifBlank { name.replace('_', ' ').replaceFirstChar(Char::uppercase) }.take(200)
    val description = property["description"].text().takeIf { it.isNotBlank() }?.take(1000)
    val default = property["default"]
    return when (property["type"].text()) {
        "string" -> {
            val choices = property["oneOf"].arrayValue().mapNotNull { option ->
                val item = option.objectValue(); val constant = item["const"]
                if (constant is JsonPrimitive && constant.isString) ElicitationChoice(constant.content, item["title"].text().ifBlank { constant.content }) else null
            }.ifEmpty { property["enum"].arrayValue().mapNotNull { (it as? JsonPrimitive)?.takeIf { value -> value.isString }?.content?.let { value -> ElicitationChoice(value, value) } } }
            if (choices.isNotEmpty()) ElicitationField(name, label, description, ElicitationKind.CHOICE, required, choices.take(ELICITATION_MAX_CHOICES), default = default)
            else ElicitationField(name, label, description, ElicitationKind.TEXT, required, format = property["format"].text().takeIf { it in setOf("email", "uri", "date", "date-time") },
                minLength = property["minLength"].intOrNull(), maxLength = property["maxLength"].intOrNull(), default = default)
        }
        "number", "integer" -> ElicitationField(name, label, description, if (property["type"].text() == "integer") ElicitationKind.INTEGER else ElicitationKind.NUMBER, required,
            minimum = property["minimum"].doubleOrNull(), maximum = property["maximum"].doubleOrNull(), default = default)
        "boolean" -> ElicitationField(name, label, description, ElicitationKind.BOOLEAN, required, default = default)
        "array" -> {
            val items = property["items"].objectValue()
            val choices = items["anyOf"].arrayValue().mapNotNull { option ->
                val item = option.objectValue(); val constant = item["const"]
                if (constant is JsonPrimitive && constant.isString) ElicitationChoice(constant.content, item["title"].text().ifBlank { constant.content }) else null
            }.ifEmpty { items["enum"].arrayValue().mapNotNull { (it as? JsonPrimitive)?.takeIf { value -> value.isString }?.content?.let { value -> ElicitationChoice(value, value) } } }
            if (choices.isEmpty()) null
            else ElicitationField(name, label, description, ElicitationKind.MULTI_CHOICE, required, choices.take(ELICITATION_MAX_CHOICES),
                minItems = property["minItems"].intOrNull(), maxItems = property["maxItems"].intOrNull(), default = default)
        }
        else -> null
    }
}

/** Starting values from schema defaults; switches start off unless a default says otherwise. */
fun ElicitationForm.initialInput(): ElicitationInput {
    val text = LinkedHashMap<String, String>(); val flags = LinkedHashMap<String, Boolean>(); val selections = LinkedHashMap<String, Set<String>>()
    for (field in fields) {
        val default = field.default as? JsonPrimitive
        when (field.kind) {
            ElicitationKind.BOOLEAN -> flags[field.name] = default?.booleanOrNull ?: false
            ElicitationKind.MULTI_CHOICE -> selections[field.name] = (field.default as? JsonArray)?.mapNotNull { it.text().takeIf { value -> field.choices.any { choice -> choice.value == value } } }?.toSet() ?: emptySet()
            ElicitationKind.CHOICE -> default?.contentOrNull?.takeIf { value -> field.choices.any { it.value == value } }?.let { text[field.name] = it }
            else -> default?.takeIf { it !is JsonNull }?.contentOrNull?.let { text[field.name] = it.take(ELICITATION_MAX_TEXT) }
        }
    }
    return ElicitationInput(text, flags, selections)
}

private val DATE = Regex("""\d{4}-\d{2}-\d{2}""")
private val DATE_TIME = Regex("""\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2}(\.\d+)?)?(Z|[+-]\d{2}:\d{2})""")
private val EMAIL = Regex("""[^@\s]+@[^@\s]+\.[^@\s]+""")
private val URI = Regex("""[A-Za-z][A-Za-z0-9+.-]*:\S+""")

private fun formatNumber(value: Double): String = if (value == Math.floor(value) && !value.isInfinite() && kotlin.math.abs(value) < 1e15) value.toLong().toString() else value.toString()

/** Per-field problems, keyed by property name. An empty map means the answer can be sent. */
fun ElicitationForm.problems(input: ElicitationInput): Map<String, String> {
    val problems = LinkedHashMap<String, String>()
    for (field in fields) {
        val text = input.text[field.name].orEmpty().trim()
        val problem: String? = when (field.kind) {
            ElicitationKind.BOOLEAN -> null
            ElicitationKind.MULTI_CHOICE -> {
                val count = input.selections[field.name].orEmpty().size
                when {
                    field.required && count == 0 -> "Choose at least one"
                    count == 0 -> null
                    field.minItems != null && count < field.minItems -> "Choose at least ${field.minItems}"
                    field.maxItems != null && count > field.maxItems -> "Choose at most ${field.maxItems}"
                    else -> null
                }
            }
            else -> when {
                text.isEmpty() -> if (field.required) "Required" else null
                field.kind == ElicitationKind.CHOICE -> if (field.choices.none { it.value == text }) "Choose one of the options" else null
                field.kind == ElicitationKind.INTEGER -> text.toLongOrNull().let { value -> if (value == null) "Enter a whole number" else range(field, value.toDouble()) }
                field.kind == ElicitationKind.NUMBER -> text.toDoubleOrNull()?.takeIf { it.isFinite() }.let { value -> if (value == null) "Enter a number" else range(field, value) }
                field.minLength != null && text.length < field.minLength -> "Use at least ${field.minLength} characters"
                text.length > (field.maxLength ?: ELICITATION_MAX_TEXT) -> "Use at most ${field.maxLength ?: ELICITATION_MAX_TEXT} characters"
                field.format == "email" && !EMAIL.matches(text) -> "Enter an email address"
                field.format == "uri" && !URI.matches(text) -> "Enter a full address, such as https://example.com"
                field.format == "date" && !DATE.matches(text) -> "Use the form YYYY-MM-DD"
                field.format == "date-time" && !DATE_TIME.matches(text) -> "Use the form YYYY-MM-DDTHH:MM:SSZ"
                else -> null
            }
        }
        if (problem != null) problems[field.name] = problem
    }
    return problems
}

private fun range(field: ElicitationField, value: Double): String? = when {
    field.minimum != null && value < field.minimum -> "Use ${formatNumber(field.minimum)} or more"
    field.maximum != null && value > field.maximum -> "Use ${formatNumber(field.maximum)} or less"
    else -> null
}

/** The accepted content: only fields with a value, typed as the schema asks. Call after [problems] is empty. */
fun ElicitationForm.content(input: ElicitationInput): JsonObject = buildJsonObject {
    for (field in fields) {
        val text = input.text[field.name].orEmpty().trim()
        when (field.kind) {
            ElicitationKind.BOOLEAN -> put(field.name, input.flags[field.name] ?: false)
            ElicitationKind.MULTI_CHOICE -> input.selections[field.name].orEmpty().takeIf { it.isNotEmpty() || field.required }?.let { selected ->
                putJsonArray(field.name) { field.choices.filter { it.value in selected }.forEach { add(it.value) } }
            }
            ElicitationKind.INTEGER -> text.toLongOrNull()?.let { put(field.name, it) }
            ElicitationKind.NUMBER -> text.toDoubleOrNull()?.let { put(field.name, it) }
            else -> if (text.isNotEmpty()) put(field.name, text)
        }
    }
}

/** JSON-RPC answer to a pending form elicitation: accept (with content), decline or cancel. */
fun elicitationResponse(permission: Permission, action: String, content: JsonObject? = null): JsonObject {
    require(permission.isElicitation) { "Not an elicitation request" }
    require(action in setOf("accept", "decline", "cancel")) { "Unknown elicitation action" }
    return buildJsonObject {
        put("jsonrpc", "2.0"); put("id", permission.id)
        putJsonObject("result") { put("action", action); if (action == "accept") put("content", content ?: JsonObject(emptyMap())) }
    }
}
