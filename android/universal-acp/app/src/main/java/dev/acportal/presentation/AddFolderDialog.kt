package dev.acportal.presentation

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.outlined.ChevronRight
import androidx.compose.material.icons.outlined.Folder
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.acportal.data.FolderClient
import dev.acportal.protocol.DirectoryListing
import dev.acportal.protocol.Workspace
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

/**
 * Choose a folder anywhere on the connected computer and add it as a place to start sessions.
 * Browsing lists folder names only. Adding and removing go through the host, which keeps the
 * result in its own state and applies it to new sessions at once.
 */
@Composable fun AddFolderDialog(client:FolderClient,added:List<Workspace>,onAdded:(Workspace)->Unit,onRemoved:()->Unit,onDismiss:()->Unit) {
    val scope=rememberCoroutineScope()
    var listing by remember {mutableStateOf<DirectoryListing?>(null)}
    var loading by remember {mutableStateOf(true)}
    var busy by remember {mutableStateOf(false)}
    var error by remember {mutableStateOf<String?>(null)}
    var removing by remember {mutableStateOf<Workspace?>(null)}
    var request by remember {mutableIntStateOf(0)}
    var target by remember {mutableStateOf<String?>(null)}
    LaunchedEffect(target,request) {
        loading=true;error=null
        try {listing=client.directories(target)}
        catch(cancelled:CancellationException) {throw cancelled}
        catch(failure:Exception) {error=failure.message ?: "Could not list folders on this computer."}
        loading=false
    }
    fun act(block:suspend ()->Unit) {
        if(busy)return
        busy=true;error=null
        scope.launch {
            try {block()}
            catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {error=failure.message ?: "That did not work. Check the connection and try again."}
            busy=false
        }
    }
    val current=listing
    AlertDialog(modifier=Modifier.testTag("add-folder-dialog"),onDismissRequest=onDismiss,
        title={Text("Add a folder")},
        text={Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment=Alignment.CenterVertically) {
                if(current?.path!=null)IconButton({target=current.parent},Modifier.testTag("add-folder-up"),enabled=!busy) {Icon(Icons.AutoMirrored.Outlined.ArrowBack,"Up one folder")}
                Column(Modifier.weight(1f)) {
                    Text(if(current?.path==null)"This computer" else folderTitle("",current.path!!),style=MaterialTheme.typography.titleSmall,maxLines=1)
                    if(current?.path!=null)FolderPathText(current.path!!)
                }
            }
            error?.let {Text(it,color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodySmall)}
            if(loading)LinearProgressIndicator(Modifier.fillMaxWidth())
            LazyColumn(Modifier.heightIn(max=280.dp)) {
                items(current?.entries.orEmpty(),key={it.path}) {folder->
                    ListItem(modifier=Modifier.clickable(enabled=!busy) {target=folder.path},
                        headlineContent={Text(folderTitle(folder.name,folder.path),maxLines=1)},
                        leadingContent={Icon(Icons.Outlined.Folder,null)},
                        trailingContent={Icon(Icons.Outlined.ChevronRight,null)})
                }
                if(!loading && current!=null && current.entries.isEmpty())item {Text("No subfolders here.",Modifier.padding(vertical=12.dp),style=MaterialTheme.typography.bodyMedium)}
            }
            if(added.isNotEmpty()) {
                HorizontalDivider()
                Text("Folders you added",style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                added.forEach {folder->
                    Row(verticalAlignment=Alignment.CenterVertically) {
                        Column(Modifier.weight(1f)) {Text(folderTitle(folder.name,folder.path),maxLines=1);FolderPathText(folder.path)}
                        TextButton({removing=folder},enabled=!busy) {Text("Remove")}
                    }
                }
            }
        }},
        confirmButton={Button({current?.path?.let {path->act {onAdded(client.add(path))}}},Modifier.testTag("add-folder-confirm"),enabled=current?.path!=null && !busy && !loading) {Text(if(busy)"Adding…" else "Add this folder")}},
        dismissButton={TextButton(onDismiss) {Text("Close")}})
    removing?.let {folder->
        AlertDialog(onDismissRequest={removing=null},
            title={Text("Remove folder?")},
            text={Text("${folderTitle(folder.name,folder.path)} will no longer be available for new sessions. The folder and its files are not deleted. Running sessions keep working.")},
            confirmButton={TextButton({removing=null;act {client.remove(folder.path);onRemoved()}},Modifier.testTag("remove-folder-confirm")) {Text("Remove")}},
            dismissButton={TextButton({removing=null}) {Text("Cancel")}})
    }
}
