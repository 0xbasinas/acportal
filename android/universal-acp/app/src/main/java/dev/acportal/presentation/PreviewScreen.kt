package dev.acportal.presentation

import android.annotation.SuppressLint
import android.graphics.Bitmap
import android.graphics.Canvas
import android.webkit.CookieManager
import android.webkit.WebResourceRequest
import android.webkit.WebResourceResponse
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.outlined.ArrowBack
import androidx.compose.material.icons.automirrored.outlined.ArrowForward
import androidx.compose.material.icons.outlined.Refresh
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import dev.acportal.data.PREVIEW_HOST
import dev.acportal.data.PreviewFetcher
import dev.acportal.data.parsePreviewAddress
import java.io.ByteArrayOutputStream

/**
 * A dev server on the host's computer, shown through the host. The page is untrusted: it runs in a view
 * with no bridge to the app, cannot store anything on this phone, and every request it makes is
 * answered by the app through the host's `/v1/preview` (see PreviewFetcher), never by the network.
 * What it cannot do: forms that post, WebSocket hot reload.
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable fun PreviewScreen(
    hostLabel:String?,fetcher:PreviewFetcher,suggestedPorts:List<Int>,startPort:Int?,
    onSendAddress:(String)->Unit,onScreenshot:((ByteArray)->String?)?,onBack:()->Unit,
) {
    val context = LocalContext.current
    var port by remember {mutableStateOf(startPort ?: suggestedPorts.firstOrNull())}
    var path by remember {mutableStateOf("/")}
    var input by remember {mutableStateOf(port?.let {"localhost:$it/"} ?: "")}
    var message by remember {mutableStateOf<String?>(null)}
    var canBack by remember {mutableStateOf(false)}
    var canForward by remember {mutableStateOf(false)}
    var loading by remember {mutableStateOf(false)}
    var shown by remember {mutableStateOf(false)}
    val web = remember {WebView(context)}
    val keyboardVisible = WindowInsets.ime.getBottom(androidx.compose.ui.platform.LocalDensity.current) > 0

    fun load(newPort:Int,newPath:String) {
        port = newPort;path = newPath;fetcher.port = newPort;shown = true
        input = "localhost:$newPort$newPath"
        web.loadUrl("https://$PREVIEW_HOST$newPath")
    }
    DisposableEffect(Unit) {
        web.settings.apply {
            javaScriptEnabled = true;domStorageEnabled = false;databaseEnabled = false;allowFileAccess = false;allowContentAccess = false
            cacheMode = WebSettings.LOAD_NO_CACHE;setSupportZoom(true);builtInZoomControls = true;displayZoomControls = false
            javaScriptCanOpenWindowsAutomatically = false;mixedContentMode = WebSettings.MIXED_CONTENT_NEVER_ALLOW
            useWideViewPort = true;loadWithOverviewMode = true
        }
        CookieManager.getInstance().setAcceptThirdPartyCookies(web,false)
        web.webViewClient = object:WebViewClient() {
            override fun shouldInterceptRequest(view:WebView,request:WebResourceRequest):WebResourceResponse =
                fetcher.fetch(request.url,request.requestHeaders["Accept"],request.requestHeaders["Accept-Language"],request.method)
            override fun shouldOverrideUrlLoading(view:WebView,request:WebResourceRequest) = request.url.host != PREVIEW_HOST
            override fun onPageStarted(view:WebView,url:String,favicon:Bitmap?) {loading = true}
            override fun onPageFinished(view:WebView,url:String) {
                loading = false;canBack = view.canGoBack();canForward = view.canGoForward()
                android.net.Uri.parse(url).let {if (it.host == PREVIEW_HOST) {path = (it.encodedPath ?: "/") + (it.encodedQuery?.let {q->"?$q"} ?: "");input = "localhost:${fetcher.port}$path"}}
            }
        }
        onDispose {
            // The page keeps nothing on this phone.
            web.stopLoading();web.clearCache(true);web.clearHistory();CookieManager.getInstance().removeAllCookies(null)
            web.destroy()
        }
    }
    LaunchedEffect(Unit) {port?.let {load(it,"/")}}
    val go:()->Unit = {
        val parsed = parsePreviewAddress(input)
        val target = parsed?.first ?: port
        if (parsed == null || target == null) message = "Type a port such as 5173, or localhost:5173/path"
        else {message = null;load(target,parsed.second)}
    }

    Column(Modifier.fillMaxSize().imePadding().statusBarsPadding()) {
        ScreenHeader("Preview","${port?.let {"localhost:$it"} ?: "No page yet"} · through ${hostLabel ?: "the host"}",onBack=onBack)
        Row(Modifier.fillMaxWidth().padding(horizontal=16.dp),verticalAlignment=Alignment.CenterVertically,horizontalArrangement=Arrangement.spacedBy(4.dp)) {
            IconButton({web.goBack()},enabled=canBack) {Icon(Icons.AutoMirrored.Outlined.ArrowBack,"Back")}
            IconButton({web.goForward()},enabled=canForward) {Icon(Icons.AutoMirrored.Outlined.ArrowForward,"Forward")}
            OutlinedTextField(input,{input = it},Modifier.weight(1f).testTag("preview-address"),singleLine=true,textStyle=MaterialTheme.typography.bodyMedium,
                keyboardOptions=KeyboardOptions(keyboardType=KeyboardType.Uri,imeAction=ImeAction.Go),keyboardActions=KeyboardActions(onGo={go()}),placeholder={Text("Port, e.g. 5173")})
            IconButton({if (shown) web.reload() else go()},Modifier.testTag("preview-reload")) {Icon(Icons.Outlined.Refresh,"Reload")}
        }
        message?.let {Text(it,Modifier.padding(horizontal=24.dp,vertical=4.dp),color=MaterialTheme.colorScheme.error,style=MaterialTheme.typography.bodySmall)}
        if (loading) LinearProgressIndicator(Modifier.fillMaxWidth().padding(horizontal=16.dp))
        Box(Modifier.weight(1f).fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp)) {
            AndroidView({web},Modifier.fillMaxSize().testTag("preview-view"))
            if (!shown) Column(Modifier.align(Alignment.Center).padding(24.dp),horizontalAlignment=Alignment.CenterHorizontally,verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Text("Preview a page",style=MaterialTheme.typography.titleMedium)
                Text("Type the port a dev server runs on, on your computer.",style=MaterialTheme.typography.bodyMedium)
                suggestedPorts.forEach {p -> OutlinedButton({load(p,"/")},Modifier.testTag("preview-port-$p")) {Text("localhost:$p")}}
            }
        }
        if (!keyboardVisible) Text("Served by the host over your connection. Nothing from this page is stored on the phone.",Modifier.padding(horizontal=24.dp,vertical=4.dp),color=MaterialTheme.colorScheme.onSurfaceVariant,style=MaterialTheme.typography.bodySmall)
        if (!keyboardVisible) Row(Modifier.fillMaxWidth().padding(16.dp),horizontalArrangement=Arrangement.spacedBy(12.dp)) {
            Button({onSendAddress("http://localhost:${fetcher.port}$path")},Modifier.weight(1f).testTag("preview-send"),enabled=shown) {Text("Send to agent")}
            OutlinedButton({
                val bitmap = Bitmap.createBitmap(web.width.coerceAtLeast(1),web.height.coerceAtLeast(1),Bitmap.Config.ARGB_8888).also {web.draw(Canvas(it))}
                var quality = 70;var scaled = bitmap
                var bytes:ByteArray
                do {
                    bytes = ByteArrayOutputStream().also {scaled.compress(Bitmap.CompressFormat.JPEG,quality,it)}.toByteArray()
                    if (bytes.size > 200*1024) {scaled = Bitmap.createScaledBitmap(scaled,(scaled.width*0.75).toInt().coerceAtLeast(1),(scaled.height*0.75).toInt().coerceAtLeast(1),true);quality = 60}
                } while (bytes.size > 200*1024 && scaled.width > 120)
                message = onScreenshot?.invoke(bytes)
            },Modifier.weight(1f).testTag("preview-shot"),enabled=shown && onScreenshot != null) {Text("Screenshot")}
        }
    }
}
