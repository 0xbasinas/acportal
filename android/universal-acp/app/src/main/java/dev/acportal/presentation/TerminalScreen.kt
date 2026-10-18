package dev.acportal.presentation

import android.annotation.SuppressLint
import android.util.Base64
import android.webkit.JavascriptInterface
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.Close
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import dev.acportal.data.TerminalLink
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonPrimitive

/**
 * A WebView whose keyboard sends plain keys: no suggestions, no composing text, no auto-correct. A predictive
 * keyboard replays composition into a terminal and doubles letters, as terminal apps found long ago.
 */
private class TerminalWebView(context:android.content.Context):WebView(context) {
    override fun onCreateInputConnection(outAttrs:android.view.inputmethod.EditorInfo):android.view.inputmethod.InputConnection? {
        val connection = super.onCreateInputConnection(outAttrs)
        outAttrs.inputType = android.text.InputType.TYPE_CLASS_TEXT or android.text.InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD or android.text.InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
        outAttrs.imeOptions = outAttrs.imeOptions or android.view.inputmethod.EditorInfo.IME_FLAG_NO_EXTRACT_UI or android.view.inputmethod.EditorInfo.IME_FLAG_NO_FULLSCREEN
        return connection
    }
}

private const val PAGE = "file:///android_asset/terminal/index.html"

/** The keys a phone keyboard lacks, as the bytes a terminal expects. */
internal val TERMINAL_KEYS: List<Pair<String,String>> = listOf(
    "Esc" to "\u001b","Tab" to "\t","↑" to "\u001b[A","↓" to "\u001b[B","←" to "\u001b[D","→" to "\u001b[C",
    "|" to "|","~" to "~","/" to "/","-" to "-",
)

/**
 * A shell on the host. xterm.js draws it in a WebView that can only show this bundled page: no network,
 * no navigation. The host connection, and the token, stay in the app; the page gets bytes to draw and
 * hands back keystrokes. The keys row adds Esc, Tab, Ctrl and arrows. Leaving the screen leaves the
 * shell running on the host (it closes itself when idle); End terminal stops it.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable fun TerminalScreen(
    path:String,hostLabel:String?,
    open:suspend (rows:Int,cols:Int)->Result<String>,
    link:suspend (id:String,scope:CoroutineScope,onData:(ByteArray)->Unit,onEvent:(JsonObject)->Unit,onState:(String,String?)->Unit)->TerminalLink,
    onEnd:(String)->Unit,existingId:String?,onBack:()->Unit,
) {
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    var status by remember {mutableStateOf("Starting a shell…")}
    var problem by remember {mutableStateOf<String?>(null)}
    var ended by remember {mutableStateOf<Int?>(null)}
    var notController by remember {mutableStateOf(false)}
    var ctrl by remember {mutableStateOf(false)}
    var terminalId by remember {mutableStateOf(existingId)}
    var attached by remember {mutableStateOf<TerminalLink?>(null)}
    val web = remember {TerminalWebView(context)}
    val ready = remember {CompletableDeferred<Pair<Int,Int>>()}
    val background = MaterialTheme.colorScheme.background.toArgb()
    val foreground = MaterialTheme.colorScheme.onBackground.toArgb()
    fun hex(argb:Int) = "#%06x".format(argb and 0xFFFFFF)
    // Output goes to the page in order; each piece is base64 so any byte survives.
    val write:(ByteArray)->Unit = {bytes -> val b64 = Base64.encodeToString(bytes,Base64.NO_WRAP);web.post {web.evaluateJavascript("acp.write('$b64')",null)}}
    val currentLink = rememberUpdatedState(attached)

    DisposableEffect(Unit) {
        // Debug builds only: lets a developer inspect the bundled page from chrome://inspect.
        WebView.setWebContentsDebuggingEnabled(dev.acportal.BuildConfig.DEBUG)
        web.settings.apply {
            javaScriptEnabled = true;domStorageEnabled = false;allowFileAccess = false;allowContentAccess = false
            blockNetworkLoads = true;setSupportZoom(false);builtInZoomControls = false;javaScriptCanOpenWindowsAutomatically = false
        }
        web.webViewClient = object:WebViewClient() {
            override fun shouldOverrideUrlLoading(view:WebView,request:WebResourceRequest) = true
            override fun shouldInterceptRequest(view:WebView,request:WebResourceRequest):WebResourceResponse? =
                if (request.url.toString().startsWith("file:///android_asset/terminal/")) null else WebResourceResponse("text/plain","utf-8",null)
        }
        web.setBackgroundColor(background)
        web.addJavascriptInterface(object {
            @JavascriptInterface fun ready(rows:Int,cols:Int) { ready.complete(rows to cols) }
            @JavascriptInterface fun input(text:String) { currentLink.value?.input(text) }
            @JavascriptInterface fun resize(rows:Int,cols:Int) { currentLink.value?.resize(rows,cols) }
            @JavascriptInterface fun ctrlUsed() { ctrl = false }
        },"AcpTerm")
        web.loadUrl(PAGE)
        onDispose {
            currentLink.value?.close()
            web.removeJavascriptInterface("AcpTerm");web.destroy()
        }
    }
    LaunchedEffect(background,foreground) {web.post {web.evaluateJavascript("window.acp&&acp.theme('${hex(background)}','${hex(foreground)}')",null)}}
    LaunchedEffect(ctrl) {web.post {web.evaluateJavascript("window.acp&&acp.setCtrl($ctrl)",null)}}
    LaunchedEffect(path) {
        val (rows,cols) = ready.await()
        val id = terminalId ?: open(rows,cols).getOrElse {problem = it.message ?: "The host could not start a terminal";status = "";return@LaunchedEffect}.also {terminalId = it}
        attached = link(id,scope,write,{event->
            when (event["type"]?.jsonPrimitive?.content) {
                "hello","controller" -> notController = event["you"]?.jsonPrimitive?.booleanOrNull == false
                "error" -> if (event["code"]?.jsonPrimitive?.content == "not_controller") notController = true
                "exit" -> ended = event["code"]?.jsonPrimitive?.intOrNull ?: -1
            }
        },{state,reason->
            status = when (state) {"connecting" -> "Connecting…";"reconnecting" -> "Reconnecting… the shell keeps running on the host.";"failed" -> reason ?: "Disconnected";else -> ""}
            if (state == "failed") problem = reason
        })
        web.post {web.evaluateJavascript("acp.focus()",null)}
    }

    Column(Modifier.fillMaxSize().imePadding().statusBarsPadding()) {
        ScreenHeader("Terminal",listOfNotNull(hostLabel,path.trimEnd('/','\\').substringAfterLast('/').substringAfterLast('\\')).joinToString(" · "),onBack=onBack,action={
            terminalId?.let {id -> IconButton({onEnd(id);onBack()},Modifier.testTag("end-terminal")) {Icon(Icons.Outlined.Close,"End terminal")}}
        })
        problem?.let {Text(it,Modifier.padding(horizontal=24.dp,vertical=4.dp),color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodyMedium)}
        if (problem == null && status.isNotEmpty()) Text(status,Modifier.padding(horizontal=24.dp,vertical=2.dp),color=MaterialTheme.colorScheme.onSurfaceVariant,style=MaterialTheme.typography.bodySmall)
        ended?.let {Text("The shell ended (exit code $it). Go back and open a new terminal.",Modifier.padding(horizontal=24.dp,vertical=4.dp),style=MaterialTheme.typography.bodyMedium)}
        if (notController && ended == null) Row(Modifier.padding(horizontal=24.dp),verticalAlignment=Alignment.CenterVertically) {
            Text("Another device has the keyboard.",Modifier.weight(1f),style=MaterialTheme.typography.bodyMedium)
            TextButton({attached?.takeControl();notController = false},Modifier.testTag("take-terminal")) {Text("Take control")}
        }
        AndroidView({web},Modifier.weight(1f).fillMaxWidth().testTag("terminal-view"))
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal=8.dp,vertical=4.dp),horizontalArrangement=Arrangement.spacedBy(6.dp),verticalAlignment=Alignment.CenterVertically) {
            FilterChip(ctrl,{ctrl = !ctrl},{Text("Ctrl")},Modifier.testTag("key-ctrl"))
            TERMINAL_KEYS.forEach {(label,bytes) -> OutlinedButton({attached?.input(bytes)},Modifier.height(40.dp).testTag("key-$label"),contentPadding=PaddingValues(horizontal=12.dp)) {Text(label)}}
        }
    }
}
