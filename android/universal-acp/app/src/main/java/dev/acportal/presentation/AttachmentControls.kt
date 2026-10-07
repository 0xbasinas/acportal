package dev.acportal.presentation

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Add
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.acportal.data.*
import dev.acportal.protocol.*
import kotlinx.coroutines.*

@Composable fun AttachmentRows(attachments:List<PromptAttachment>,enabled:Boolean,onRemove:(Int)->Unit) {
    if(attachments.isNotEmpty())Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
        attachments.forEachIndexed {index,attachment->Surface(shape=RoundedCornerShape(10.dp),color=MaterialTheme.colorScheme.surfaceContainerHigh) {
            Row(Modifier.widthIn(max=240.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(attachment.name,Modifier.padding(start=12.dp).weight(1f,false),style=MaterialTheme.typography.bodySmall,maxLines=1,overflow=TextOverflow.Ellipsis)
                IconButton({onRemove(index)},enabled=enabled) {Icon(Icons.Outlined.Close,"Remove attachment ${index+1}: ${attachment.name}",Modifier.size(18.dp))}
            }
        }}
    }
}

@Composable fun AttachmentControls(info:SessionInfo,enabled:Boolean,resetVersion:Long,onLoading:(Boolean)->Unit,onAdd:(PromptAttachment,Long)->Unit,onHistory:(()->Unit)?=null) {
    var menu by remember {mutableStateOf(false)}
    var uriDialog by remember {mutableStateOf(false)}
    var uri by remember {mutableStateOf("")}
    var failure by remember {mutableStateOf<String?>(null)}
    var loading by remember {mutableStateOf(false)}
    var selectedKind by remember {mutableStateOf<AttachmentKind?>(null)}
    var selectedVersion by remember {mutableLongStateOf(resetVersion)}
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    LaunchedEffect(resetVersion) {uri="";uriDialog=false;menu=false;failure=null}
    val picker=rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) {document->
        val kind=selectedKind;val version=selectedVersion;selectedKind=null
        if(document!=null && kind!=null)scope.launch {
            loading=true;onLoading(true)
            try {onAdd(loadAttachment(context,document,kind),version)}
            catch(cancelled:CancellationException) {throw cancelled}
            catch(error:IllegalArgumentException) {failure=error.message ?: "This file could not be attached."}
            catch(_:Exception) {failure="This file could not be read. Choose it again or select another file."}
            finally {loading=false;onLoading(false)}
        }
    }
    Box {
        IconButton({menu=true},enabled=(enabled || onHistory!=null) && !loading) {Icon(Icons.Outlined.Add,"Add context",Modifier.size(22.dp))}
        DropdownMenu(menu,{menu=false}) {
            fun select(kind:AttachmentKind,mime:String) {menu=false;selectedKind=kind;selectedVersion=resetVersion;picker.launch(arrayOf(mime))}
            if(info.promptCapability("embeddedContext"))DropdownMenuItem(text={Text("Attach file")},enabled=enabled,onClick={select(AttachmentKind.FILE,"*/*")})
            if(info.promptCapability("image"))DropdownMenuItem(text={Text("Attach image")},enabled=enabled,onClick={select(AttachmentKind.IMAGE,"image/*")})
            if(info.promptCapability("audio"))DropdownMenuItem(text={Text("Attach audio")},enabled=enabled,onClick={select(AttachmentKind.AUDIO,"audio/*")})
            DropdownMenuItem(text={Text("Context URI")},enabled=enabled,onClick={menu=false;uri="";failure=null;uriDialog=true})
            onHistory?.let {callback->DropdownMenuItem(text={Text("Prompt history")},onClick={menu=false;callback()})}
        }
    }
    if(loading)CircularProgressIndicator(Modifier.size(18.dp),strokeWidth=2.dp)
    if(uriDialog)AlertDialog(onDismissRequest={uriDialog=false},title={Text("Add context reference")},text={Column(verticalArrangement=Arrangement.spacedBy(12.dp)) {
        Text("Reference a file or resource the agent can access on the host.",style=MaterialTheme.typography.bodyMedium)
        OutlinedTextField(uri,{uri=it;failure=null},label={Text("Context URI")},placeholder={Text("file:///workspace/src/main.kt")},singleLine=true)
        failure?.let {Text(it,color=MaterialTheme.colorScheme.error)}
    }},confirmButton={TextButton({try {onAdd(contextReference(uri),resetVersion);uriDialog=false;uri=""}catch(error:IllegalArgumentException) {failure=error.message}} ,enabled=enabled && uri.isNotBlank()) {Text("Add reference")}},dismissButton={TextButton({uriDialog=false;uri="";failure=null}) {Text("Cancel")}})
    else failure?.let {message->AlertDialog(onDismissRequest={failure=null},title={Text("Attachment unavailable")},text={Text(message)},confirmButton={TextButton({failure=null}) {Text("OK")}})}
}
