package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import dev.acportal.protocol.EarlierSessions

/** What an agent remembers for the chosen folder. Listing is read-only; only a tap opens one. */
@Composable fun EarlierSessionsDialog(agentName:String,load:suspend ()->EarlierSessions?,onDismiss:()->Unit,onOpen:(String)->Unit) {
    var result by remember {mutableStateOf<EarlierSessions?>(null)}
    var failed by remember {mutableStateOf(false)}
    LaunchedEffect(Unit) {
        val loaded=load()
        if(loaded==null)failed=true else result=loaded
    }
    Dialog(onDismissRequest=onDismiss) {
        Surface(shape=MaterialTheme.shapes.large,color=MaterialTheme.colorScheme.surfaceContainer) {
            Column(Modifier.fillMaxWidth().heightIn(max=560.dp).padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Row(Modifier.fillMaxWidth(),verticalAlignment=Alignment.CenterVertically) {
                    Text("Earlier sessions",Modifier.weight(1f).semantics {heading()},style=MaterialTheme.typography.titleLarge)
                    TextButton(onDismiss,Modifier.heightIn(min=48.dp)) {Text("Close")}
                }
                val shown=result
                when {
                    failed -> Text("Could not read $agentName's sessions.",style=MaterialTheme.typography.bodyMedium)
                    shown==null -> Text("Asking $agentName…",style=MaterialTheme.typography.bodyMedium)
                    !shown.listSupported -> Text("$agentName does not list its earlier sessions.",style=MaterialTheme.typography.bodyMedium)
                    shown.sessions.isEmpty() -> Text("Nothing remembered for this folder.",style=MaterialTheme.typography.bodyMedium)
                    else -> {
                        if(!shown.loadSupported)Text("$agentName cannot reopen them, so this list is for reference.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                        LazyColumn(Modifier.testTag("earlier-sessions-list")) {
                            items(shown.sessions,key={it.sessionId}) {session->
                                Column(Modifier.fillMaxWidth().clickable(enabled=shown.loadSupported) {onOpen(session.sessionId)}.padding(vertical=12.dp).heightIn(min=48.dp).testTag("earlier:${session.sessionId}")) {
                                    Text(session.title?.takeIf {it.isNotBlank()} ?: session.sessionId,style=MaterialTheme.typography.bodyLarge,maxLines=2)
                                    session.updatedAt?.takeIf {it.isNotBlank()}?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
