package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.role
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*

/**
 * What a new session may do without asking, as the host describes it (`GET /v1/presets`). Only
 * the host decides which choices exist: "Auto-approve everything" appears only where the host
 * allows it for the agent, and is never remembered for the next session.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable internal fun PermissionsRow(agentId:String,selected:String,onSelect:(String)->Unit,load:suspend (String)->List<PermissionPreset>?) {
    var presets by remember(agentId) {mutableStateOf<List<PermissionPreset>?>(null)}
    var open by rememberSaveable {mutableStateOf(false)}
    LaunchedEffect(agentId) {presets=if(agentId.isBlank())null else load(agentId)}
    // A choice the chosen agent does not offer falls back to the plain one.
    LaunchedEffect(presets) {presets?.let {list->if(list.none {it.id==selected})onSelect("supervised")}}
    val list=presets ?: return
    if(list.size<2)return
    val current=list.firstOrNull {it.id==selected} ?: list.first()
    Row(Modifier.fillMaxWidth().heightIn(min=56.dp).clickable {open=true}.padding(horizontal=24.dp,vertical=8.dp).testTag("permissions-row"),verticalAlignment=Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("Permissions",style=MaterialTheme.typography.titleMedium)
            Text(current.label,style=MaterialTheme.typography.bodySmall,color=if(current.id==PRESET_YOLO)MaterialTheme.colorScheme.tertiary else MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Icon(Icons.Outlined.ChevronRight,null,tint=MaterialTheme.colorScheme.onSurfaceVariant)
    }
    if(open)ModalBottomSheet(onDismissRequest={open=false},containerColor=MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(horizontal=24.dp).padding(bottom=24.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text("Permissions",style=MaterialTheme.typography.titleLarge)
            list.forEach {preset->
                val on=preset.id==current.id
                val yolo=preset.id==PRESET_YOLO
                Surface(Modifier.fillMaxWidth().testTag("preset-${preset.id}").clickable(role=Role.RadioButton) {onSelect(preset.id);open=false},shape=RoundedCornerShape(12.dp),color=if(on && yolo)MaterialTheme.colorScheme.tertiaryContainer else MaterialTheme.colorScheme.surfaceContainer) {
                    Row(Modifier.padding(12.dp),verticalAlignment=Alignment.Top) {
                        RadioButton(on,null)
                        Column(Modifier.padding(start=12.dp),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                            Text(preset.label,style=MaterialTheme.typography.titleSmall)
                            if(on || preset.effects.size<=1)preset.effects.forEach {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                            else preset.effects.firstOrNull()?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1)}
                        }
                    }
                }
            }
            Button({open=false},Modifier.fillMaxWidth().heightIn(min=52.dp)) {Text("Done")}
        }
    }
}
