package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.PromptHistoryEntry

@OptIn(ExperimentalMaterial3Api::class)
@Composable fun PromptHistorySheet(entries:List<PromptHistoryEntry>,onDismiss:()->Unit,onSelect:(String)->Unit) {
    ModalBottomSheet(onDismissRequest=onDismiss,containerColor=MaterialTheme.colorScheme.surfaceContainerLow) {
        Column(Modifier.fillMaxWidth().heightIn(max=520.dp)) {
            ScreenHeader("Prompt history")
            Text("Choose text to place in your draft. Send when ready. Past attachments are not copied.",Modifier.padding(horizontal=24.dp),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(entries.isEmpty())Text("No submitted text prompts in this conversation's retained history.",Modifier.padding(24.dp),style=MaterialTheme.typography.bodyMedium)
            else LazyColumn(Modifier.weight(1f,false).testTag("prompt-history"),contentPadding=PaddingValues(vertical=12.dp)) {
                items(entries,key={it.id}) {entry->
                    Text(entry.text.take(256),Modifier.fillMaxWidth().testTag("history:${entry.id}").clickable {onSelect(entry.text)}.padding(horizontal=24.dp,vertical=18.dp).heightIn(min=48.dp),style=MaterialTheme.typography.bodyMedium,maxLines=3,overflow=TextOverflow.Ellipsis)
                }
            }
            Spacer(Modifier.navigationBarsPadding().height(16.dp))
        }
    }
}
