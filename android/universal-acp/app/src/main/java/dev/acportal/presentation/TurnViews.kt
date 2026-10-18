package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.ExpandMore
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import kotlinx.coroutines.delay

/** The id of the newest user message: the turn that is running or has just finished. */
internal fun lastUserId(items:List<TimelineItem>):String? {
    for(index in items.indices.reversed()) {val item=items[index];if(item is TimelineItem.Text && item.role=="user")return item.id}
    return null
}

/** A finished turn's steps folded into one line: "Worked for 1m 12s · 5 steps". */
@Composable internal fun StepsGroup(took:Long?,block:TurnBlock.Steps,open:Boolean,onToggle:(Boolean)->Unit,content:@Composable ()->Unit) {
    Column(Modifier.fillMaxWidth().testTag("steps-group")) {
        TextButton({onToggle(!open)},Modifier.heightIn(min=48.dp).semantics {stateDescription=if(open)"Expanded" else "Collapsed"}.testTag("steps-toggle-${block.id}"),contentPadding=PaddingValues(horizontal=4.dp)) {
            Icon(if(open)Icons.Outlined.ExpandMore else Icons.Outlined.ChevronRight,if(open)"Hide steps" else "Show steps",Modifier.size(18.dp))
            Spacer(Modifier.width(8.dp))
            Text((if(took!=null)"Worked for ${formatDuration(took)}" else "Worked")+" · "+stepsLabel(block),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if(open)Column(Modifier.padding(start=12.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {content()}
    }
}

/** The running turn's last row: the agent is working, and for how long this phone has watched. */
@Composable internal fun WorkingRow(agent:String,since:Long?) {
    var now by remember {mutableLongStateOf(System.currentTimeMillis())}
    LaunchedEffect(since) {while(since!=null) {delay(1000);now=System.currentTimeMillis()}}
    Row(Modifier.fillMaxWidth().padding(vertical=4.dp).testTag("working-row"),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        CircularProgressIndicator(Modifier.size(12.dp),strokeWidth=2.dp)
        Text(agent+" is working…",style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurface)
        if(since!=null)Text(formatDuration(now-since),style=MaterialTheme.typography.labelLarge,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}
