package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.error
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import kotlinx.serialization.json.JsonObject

/**
 * A form elicitation from the agent. Nothing is sent until the user picks Submit, Decline or
 * Cancel; dismissing the sheet leaves the question pending, like a permission.
 */
@Composable fun ElicitationRequest(permission:Permission,enabled:Boolean=true,onAnswer:(String,JsonObject?)->Unit) {
    val form=remember(permission) {elicitationForm(permission)}
    var input by remember(permission.id) {mutableStateOf(form.initialInput())}
    var attempted by rememberSaveable(permission.id.toString()) {mutableStateOf(false)}
    val problems=remember(form,input) {form.problems(input)}
    Column(Modifier.fillMaxWidth().padding(horizontal=24.dp,vertical=16.dp).testTag("elicitation-form"),verticalArrangement=Arrangement.spacedBy(12.dp)) {
        Text("The agent is asking you",Modifier.semantics {heading()},style=MaterialTheme.typography.labelLarge)
        Text(form.message.ifBlank {"The agent needs more information"},style=MaterialTheme.typography.titleLarge)
        form.title?.let {Text(it,style=MaterialTheme.typography.titleMedium)}
        form.description?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        Text("Your answer goes only to this agent. Don't enter passwords or keys here.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        if(!form.canAccept)Text("This form needs fields the app can't show (${form.unsupportedRequired.joinToString(", ").take(200)}). You can decline or cancel.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.error)
        form.fields.forEach {field->
            val problem=problems[field.name]?.takeIf {attempted}
            ElicitationFieldInput(field,input,enabled,problem) {input=it}
        }
        Spacer(Modifier.height(8.dp))
        Button({attempted=true;if(problems.isEmpty() && form.canAccept)onAnswer("accept",form.content(input))},enabled=enabled && form.canAccept,modifier=Modifier.fillMaxWidth().heightIn(min=52.dp).testTag("elicitation-submit"),shape=RoundedCornerShape(10.dp)) {Text("Submit")}
        if(attempted && problems.isNotEmpty())Text("Fix the highlighted fields to submit.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
        TextButton({onAnswer("decline",null)},enabled=enabled,modifier=Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("elicitation-decline")) {Text("Decline")}
        TextButton({onAnswer("cancel",null)},enabled=enabled,modifier=Modifier.fillMaxWidth().heightIn(min=48.dp).testTag("elicitation-cancel")) {Text("Cancel")}
    }
}

@Composable private fun ElicitationFieldInput(field:ElicitationField,input:ElicitationInput,enabled:Boolean,problem:String?,onChange:(ElicitationInput)->Unit) {
    val label=if(field.required)"${field.label} (required)" else field.label
    val errorModifier=if(problem!=null)Modifier.semantics {error(problem)} else Modifier
    Column(Modifier.fillMaxWidth().testTag("elicitation-field:${field.name}"),verticalArrangement=Arrangement.spacedBy(4.dp)) {
        when(field.kind) {
            ElicitationKind.BOOLEAN -> {
                val checked=input.flags[field.name] ?: false
                Row(Modifier.fillMaxWidth().heightIn(min=48.dp).toggleable(checked,enabled=enabled,role=Role.Switch) {onChange(input.copy(flags=input.flags+(field.name to it)))}.then(errorModifier),verticalAlignment=Alignment.CenterVertically) {
                    Text(label,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
                    Switch(checked,null,enabled=enabled)
                }
            }
            ElicitationKind.CHOICE -> {
                Text(label,style=MaterialTheme.typography.bodyLarge)
                val selected=input.text[field.name]
                field.choices.forEach {choice->
                    Row(Modifier.fillMaxWidth().heightIn(min=48.dp).selectable(selected==choice.value,enabled=enabled,role=Role.RadioButton) {onChange(input.copy(text=input.text+(field.name to choice.value)))},verticalAlignment=Alignment.CenterVertically) {
                        RadioButton(selected==choice.value,null,enabled=enabled);Spacer(Modifier.width(12.dp));Text(choice.label,style=MaterialTheme.typography.bodyMedium)
                    }
                }
                if(!field.required && selected!=null)TextButton({onChange(input.copy(text=input.text-field.name))},enabled=enabled) {Text("Clear choice")}
            }
            ElicitationKind.MULTI_CHOICE -> {
                Text(label,style=MaterialTheme.typography.bodyLarge)
                val selected=input.selections[field.name].orEmpty()
                field.choices.forEach {choice->
                    val checked=choice.value in selected
                    Row(Modifier.fillMaxWidth().heightIn(min=48.dp).toggleable(checked,enabled=enabled,role=Role.Checkbox) {onChange(input.copy(selections=input.selections+(field.name to if(it)selected+choice.value else selected-choice.value)))},verticalAlignment=Alignment.CenterVertically) {
                        Checkbox(checked,null,enabled=enabled);Spacer(Modifier.width(12.dp));Text(choice.label,style=MaterialTheme.typography.bodyMedium)
                    }
                }
            }
            else -> {
                val keyboard=when {
                    field.kind==ElicitationKind.INTEGER -> KeyboardType.Number
                    field.kind==ElicitationKind.NUMBER -> KeyboardType.Decimal
                    field.format=="email" -> KeyboardType.Email
                    field.format=="uri" -> KeyboardType.Uri
                    else -> KeyboardType.Text
                }
                val hint=when {
                    field.format=="date" -> "YYYY-MM-DD"
                    field.format=="date-time" -> "YYYY-MM-DDTHH:MM:SSZ"
                    else -> {val low=field.minimum;val high=field.maximum;if(low!=null && high!=null)"${plainNumber(low)} to ${plainNumber(high)}" else null}
                }
                OutlinedTextField(input.text[field.name].orEmpty(),{value->onChange(input.copy(text=input.text+(field.name to value.take(ELICITATION_MAX_TEXT))))},Modifier.fillMaxWidth().then(errorModifier),enabled=enabled,
                    label={Text(label)},placeholder=hint?.let {{Text(it)}},isError=problem!=null,singleLine=field.kind!=ElicitationKind.TEXT || field.format!=null,
                    keyboardOptions=KeyboardOptions(keyboardType=keyboard),supportingText=problem?.let {{Text(it)}})
            }
        }
        field.description?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        if(problem!=null && field.kind in setOf(ElicitationKind.BOOLEAN,ElicitationKind.CHOICE,ElicitationKind.MULTI_CHOICE))Text(problem,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
    }
}

private fun plainNumber(value:Double):String = if(value%1.0==0.0 && kotlin.math.abs(value)<1e15)value.toLong().toString() else value.toString()
