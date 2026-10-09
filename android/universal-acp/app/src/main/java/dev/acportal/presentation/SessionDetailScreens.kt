package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.selection.selectable
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.platform.testTag
import dev.acportal.protocol.*
import kotlinx.serialization.json.*

@Composable fun AgentDetailsScreen(info:SessionInfo,onBack:()->Unit,hostLabel:String?=null,usage:JsonObject=JsonObject(emptyMap()),sessionInfo:JsonObject=JsonObject(emptyMap())) {
    val capabilities=info.initialization["agentCapabilities"].objectValue()
    val name=info.initialization["agentInfo"].objectValue()["name"].text().ifBlank {info.agentId}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Agent details",onBack=onBack)
        Column(Modifier.padding(horizontal=24.dp).verticalScroll(rememberScrollState()),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text(name,style=MaterialTheme.typography.titleLarge)
            Text(hostLabel ?: info.workspace.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\'),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(16.dp))
            DetailValue("Session setup",if(info.acpSessionId.isBlank())"Sign-in required" else "Complete")
            if(info.ownTools)OwnToolsNotice()
            sessionInfo["title"].text().takeIf {it.isNotBlank()}?.let {DetailValue("Session title",it)}
            sessionInfo["updatedAt"].text().takeIf {it.isNotBlank()}?.let {DetailValue("Agent updated",it)}
            val used=(usage["used"] as? JsonPrimitive)?.longOrNull
            val size=(usage["size"] as? JsonPrimitive)?.longOrNull
            if(used!=null && size!=null && used>=0 && size>=0)DetailValue("Context tokens","$used / $size")
            val cost=usage["cost"].objectValue()
            val amount=(cost["amount"] as? JsonPrimitive)?.doubleOrNull
            val currency=cost["currency"].text()
            if(amount!=null && amount.isFinite() && amount>=0 && Regex("[A-Z]{3}").matches(currency))DetailValue("Agent-reported cost","${cost["amount"].text()} $currency")
            DetailValue("Session history",if(capabilities["loadSession"].flag())"Supported" else "Unavailable")
            DetailValue("Image prompts",if(capabilities["promptCapabilities"].objectValue()["image"].flag())"Supported" else "Unavailable")
            DetailValue("Audio prompts",if(capabilities["promptCapabilities"].objectValue()["audio"].flag())"Supported" else "Unavailable")
            DetailValue("Embedded context",if(capabilities["promptCapabilities"].objectValue()["embeddedContext"].flag())"Supported" else "Unavailable")
            DetailValue("MCP over HTTP",if(capabilities["mcpCapabilities"].objectValue()["http"].flag())"Supported" else "Unavailable")
            DetailValue("MCP over SSE",if(capabilities["mcpCapabilities"].objectValue()["sse"].flag())"Supported" else "Unavailable")
            Spacer(Modifier.height(16.dp))
            Text("Capabilities reported by this agent during initialization.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            info.authenticationMethods().forEach {method->Text(method["name"].text(),style=MaterialTheme.typography.bodyMedium)}
        }
    }
}

@Composable private fun DetailValue(label:String,value:String) {
    Row(Modifier.fillMaxWidth().padding(vertical=10.dp),horizontalArrangement=Arrangement.SpaceBetween,verticalAlignment=Alignment.CenterVertically) {
        Text(label,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
        Text(value,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable fun SessionOptionsScreen(info:SessionInfo,state:SessionState,enabled:Boolean,onBack:()->Unit,onConfigure:(String,JsonObject)->Unit) {
    val configuration=remember(state.configOptions) {state.configOptions.map {it.objectValue()}.filter {it["type"].text()=="select"}}
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Session options",onBack=onBack)
        if(state.permissions.isNotEmpty())TextButton(onBack,Modifier.padding(horizontal=24.dp)) {Text(if(state.permissions.values.first().isElicitation)"Return to the agent's question" else "Return to pending permission")}
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal=24.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            Text(state.items.filterIsInstance<TimelineItem.Text>().firstOrNull {it.role=="user"}?.text?.lineSequence()?.firstOrNull()?.take(80) ?: info.agentId,style=MaterialTheme.typography.titleMedium)
            Text("${info.agentId} · ${info.workspace.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\')}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(8.dp))
            if(configuration.isNotEmpty())configuration.forEach {option->
                ConfigSelector(option,enabled) {value->onConfigure("session/set_config_option",buildJsonObject {put("configId",option["id"].text());put("value",value)})}
            } else {
                val models=state.models["availableModels"].arrayValue()
                val modelOption=remember(state.models) {buildJsonObject {put("name","Model");put("currentValue",state.models["currentModelId"] ?: JsonNull);putJsonArray("options") {models.forEach {value->add(JsonObject(value.objectValue()+ ("value" to (value.objectValue()["modelId"] ?: JsonNull))))}}}}
                if(models.isNotEmpty())ConfigSelector(modelOption,enabled) {model->onConfigure("session/set_model",buildJsonObject {put("modelId",model)})}
                val modes=state.modes["availableModes"].arrayValue()
                if(modes.isNotEmpty()) {
                    Text("Mode",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    modes.forEach {value->val mode=value.objectValue();Row(Modifier.fillMaxWidth().clickable(enabled=enabled) {onConfigure("session/set_mode",buildJsonObject {put("modeId",mode["id"].text())})},verticalAlignment=Alignment.CenterVertically) {
                        Text(mode["name"].text(),Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge)
                        RadioButton(state.modes["currentModeId"]==mode["id"],onClick={onConfigure("session/set_mode",buildJsonObject {put("modeId",mode["id"].text())})},enabled=enabled)
                    } }
                }
                if(models.isEmpty() && modes.isEmpty())Text("This agent has not supplied session options.",style=MaterialTheme.typography.bodyMedium)
            }
            Text("Available options come from the connected agent.",Modifier.padding(top=24.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
        TextButton(onBack,Modifier.fillMaxWidth().padding(24.dp).heightIn(min=48.dp)) {Text("Done")}
    }
}

@Composable private fun ConfigSelector(option:JsonObject,enabled:Boolean,onSelect:(String)->Unit) {
    var show by remember(option["id"],option["name"]) {mutableStateOf(false)}
    val groups=remember(option["options"]) {option["options"].arrayValue().map {it.objectValue()}}
    val values=remember(groups) {groups.flatMap {group->if(group["options"] is JsonArray)group["options"].arrayValue().map {it.objectValue()} else listOf(group)}}
    LaunchedEffect(enabled) {if(!enabled)show=false}
    val current=values.find {it["value"]==option["currentValue"]}?.get("name").text().ifBlank {option["currentValue"].text()}
    Column {
        Text(option["name"].text(),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        Row(Modifier.fillMaxWidth().clickable(enabled=enabled && values.isNotEmpty()) {show=true}.padding(vertical=16.dp),verticalAlignment=Alignment.CenterVertically) {Text(current,Modifier.weight(1f),style=MaterialTheme.typography.bodyLarge);Icon(Icons.Outlined.ExpandMore,"Choose ${option["name"].text()}")}
        option["description"].text().takeIf {it.isNotBlank()}?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
        if(values.size>40) {
            if(show)LargeConfigPicker(option,groups,enabled,{show=false}) {value->show=false;onSelect(value)}
        } else DropdownMenu(show,{show=false}) {groups.forEach {group->
            if(group["options"] is JsonArray) {
                Text(group["name"].text(),Modifier.padding(horizontal=16.dp,vertical=8.dp),style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                group["options"].arrayValue().forEach {value->val choice=value.objectValue();DropdownMenuItem(text={Text(choice["name"].text())},onClick={show=false;onSelect(choice["value"].text())})}
            } else DropdownMenuItem(text={Text(group["name"].text())},onClick={show=false;onSelect(group["value"].text())})
        } }
    }
}

private data class ConfigChoice(val value:String,val name:String,val group:String)

/** Large catalogs compose visible rows only. Opening or filtering sends no mutation. */
@Composable private fun LargeConfigPicker(option:JsonObject,groups:List<JsonObject>,enabled:Boolean,onDismiss:()->Unit,onSelect:(String)->Unit) {
    var query by remember {mutableStateOf("")}
    val choices=remember(groups) {groups.flatMap {group->
        if(group["options"] is JsonArray)group["options"].arrayValue().map {value->val choice=value.objectValue();ConfigChoice(choice["value"].text(),choice["name"].text(),group["name"].text())}
        else listOf(ConfigChoice(group["value"].text(),group["name"].text(),""))
    }}
    val filtered=remember(choices,query) {choices.filter {query.isBlank() || it.name.contains(query,true) || it.value.contains(query,true) || it.group.contains(query,true)}}
    Dialog(onDismissRequest=onDismiss) {
        Surface(shape=MaterialTheme.shapes.large,color=MaterialTheme.colorScheme.surfaceContainer) {
            Column(Modifier.fillMaxWidth().heightIn(max=560.dp).padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Text("Choose ${option["name"].text()}",Modifier.semantics {heading()},style=MaterialTheme.typography.titleLarge)
                OutlinedTextField(query,{query=it.take(512)},label={Text("Search options")},singleLine=true,modifier=Modifier.fillMaxWidth())
                Text("Options: ${filtered.size}",style=MaterialTheme.typography.bodySmall)
                LazyColumn(Modifier.weight(1f).fillMaxWidth().testTag("config-choice-list")) {
                    itemsIndexed(filtered,key={index,_->index}) {_,choice->
                        Row(Modifier.fillMaxWidth().heightIn(min=48.dp).selectable(selected=choice.value==option["currentValue"].text(),enabled=enabled,role=Role.RadioButton,onClick={onSelect(choice.value)}).padding(vertical=8.dp),verticalAlignment=Alignment.CenterVertically) {
                            Column(Modifier.weight(1f)) {Text(choice.name.ifBlank {choice.value},style=MaterialTheme.typography.bodyLarge);if(choice.group.isNotBlank())Text(choice.group,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                            RadioButton(choice.value==option["currentValue"].text(),onClick=null,enabled=enabled)
                        }
                    }
                    if(filtered.isEmpty())item {Text("No matching options. Try another search.")}
                }
                TextButton(onDismiss,Modifier.align(Alignment.End)) {Text("Cancel")}
            }
        }
    }
}
