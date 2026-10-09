package dev.acportal.presentation

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import androidx.core.net.toUri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.LifecycleEventObserver
import androidx.lifecycle.compose.LocalLifecycleOwner
import dev.acportal.data.*
import dev.acportal.protocol.*
import kotlinx.coroutines.*
import kotlinx.serialization.json.*

@Composable fun ReceivedContentView(block:JsonObject) {
    val context=LocalContext.current
    val scope=rememberCoroutineScope()
    val kind=block["type"].text()
    val resource=contentResource(block)
    val mime=receivedContentMime(block)
    val name=contentName(block)
    var error by remember(block) {mutableStateOf<String?>(null)}
    var saving by remember(block) {mutableStateOf(false)}
    var pendingSave by remember {mutableStateOf<JsonObject?>(null)}
    var saved by remember(block) {mutableStateOf(false)}
    var expanded by remember(block) {mutableStateOf(false)}
    var preview by remember(block) {mutableStateOf(false)}
    var openLink by remember(block) {mutableStateOf(false)}
    val destination=rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(mime)) {uri->
        val captured=pendingSave;pendingSave=null
        if(uri!=null && captured!=null)scope.launch {
            saving=true
            try {saveReceivedContent(context,uri,captured);saved=true;error=null}
            catch(cancelled:CancellationException) {throw cancelled}
            catch(_:Exception) {error="Content could not be saved. Check the data and choose a writable destination."}
            finally {saving=false}
        }
    }
    if(kind=="text") {SelectionContainer {Text(block["text"].text(),style=MaterialTheme.typography.bodyMedium)};return}
    Column(Modifier.fillMaxWidth(),verticalArrangement=Arrangement.spacedBy(8.dp)) {
        Text(name,style=MaterialTheme.typography.titleSmall)
        Text(when(kind) {"image"->"Image · $mime";"audio"->"Audio · $mime";"resource"->"Embedded resource · $mime";"resource_link"->"Resource reference";else->"This content type is not supported yet."},style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        val uri=resource["uri"].text()
        if(uri.isNotBlank()) {
            SelectionContainer {Text(uri,style=MaterialTheme.typography.bodySmall)}
            Row(horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                TextButton({(context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).setPrimaryClip(ClipData.newPlainText("Resource URI",uri))}) {Text("Copy URI")}
                if(contentWebUrl(block)!=null)TextButton({openLink=true}) {Text("Open link")}
            }
        }
        if(kind=="resource_link")resource["description"].text().takeIf {it.isNotBlank()}?.let {Text(it,style=MaterialTheme.typography.bodySmall)}
        if(kind=="resource" && resource["text"] is JsonPrimitive) {
            TextButton({expanded=!expanded}) {Text(if(expanded)"Hide resource text" else "Show resource text")}
            if(expanded)Column(Modifier.heightIn(max=280.dp).verticalScroll(rememberScrollState())) {TerminalOutput(resource["text"].text());if(resource["text"].text().length>64000)Text("Preview limited to 64,000 characters. Save the resource to read it in full.",style=MaterialTheme.typography.bodySmall)}
        }
        if(kind=="image")TextButton({preview=true}) {Text("Preview image")}
        if(kind=="audio")AudioContentControls(block)
        val embedded=kind in listOf("image","audio") || kind=="resource" && (resource["text"] is JsonPrimitive || resource["blob"] is JsonPrimitive)
        if(embedded)TextButton({pendingSave=block;destination.launch(receivedContentFilename(block))},enabled=!saving) {Text(if(saving)"Saving…" else "Save content")}
        if(saved)Text("Content saved",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        error?.let {Text(it,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)}
    }
    if(preview)ImageContentPreview(block,{preview=false})
    if(openLink)AlertDialog(onDismissRequest={openLink=false},title={Text("Open resource link?")},text={Text("This opens the agent-provided address in another app.\n\n${contentWebUrl(block).orEmpty()}")},confirmButton={TextButton({openLink=false;try {context.startActivity(Intent(Intent.ACTION_VIEW,contentWebUrl(block)!!.toUri()))} catch(_:Exception) {error="No app could open this link. You can copy its URI instead."}}) {Text("Open")}},dismissButton={TextButton({openLink=false}) {Text("Cancel")}})
}

@Composable private fun ImageContentPreview(block:JsonObject,onClose:()->Unit) {
    var bitmap by remember(block) {mutableStateOf<android.graphics.Bitmap?>(null)}
    var failure by remember(block) {mutableStateOf<String?>(null)}
    LaunchedEffect(block) {try {bitmap=receivedImagePreview(block)} catch(cancelled:CancellationException) {throw cancelled} catch(error:Exception) {failure=error.message ?: "The image could not be previewed."}}
    Dialog(onDismissRequest=onClose) {
        Surface(shape=MaterialTheme.shapes.large) {
            Column(Modifier.fillMaxWidth().padding(16.dp),verticalArrangement=Arrangement.spacedBy(12.dp)) {
                Text(contentName(block),style=MaterialTheme.typography.titleMedium)
                if(bitmap!=null)Image(bitmap!!.asImageBitmap(),"Received image preview",Modifier.fillMaxWidth().heightIn(max=480.dp),contentScale=ContentScale.Fit)
                else if(failure!=null)Text(failure!!,style=MaterialTheme.typography.bodySmall)
                else CircularProgressIndicator()
                TextButton(onClose) {Text("Close preview")}
            }
        }
    }
}

@Composable private fun AudioContentControls(block:JsonObject) {
    val context=LocalContext.current
    val lifecycle=LocalLifecycleOwner.current.lifecycle
    val scope=rememberCoroutineScope()
    val player=remember(block) {ReceivedAudioPlayer(context)}
    val state by player.state.collectAsState()
    DisposableEffect(player,lifecycle) {
        val observer=LifecycleEventObserver {_,event->if(event==Lifecycle.Event.ON_STOP)player.stop()}
        lifecycle.addObserver(observer)
        onDispose {lifecycle.removeObserver(observer);player.stop()}
    }
    TextButton({if(state in listOf(AudioPlaybackState.LOADING,AudioPlaybackState.PLAYING))player.stop() else scope.launch {player.play(block)}}) {Text(when(state) {AudioPlaybackState.LOADING->"Cancel audio";AudioPlaybackState.PLAYING->"Stop audio";else->"Play audio"})}
    if(state==AudioPlaybackState.FAILED)Text("This audio could not be played. Save it to try another player.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error)
}
