package dev.acportal.data

import android.webkit.WebResourceResponse
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.ByteArrayInputStream

/** The made-up address the preview page lives at. Nothing resolves it; the app answers every request for it itself. */
const val PREVIEW_HOST = "preview.acportal.invalid"

/**
 * Answers the preview page's requests through the host (`GET /v1/preview/{port}/{path}`) with this
 * phone's credential, which never reaches the page. Anything the page asks for outside its own
 * address gets an empty refusal, so a page cannot contact the network from the phone.
 */
class PreviewFetcher(private val client:OkHttpClient,private val address:String,private val token:String) {
    @Volatile var port:Int = 0

    /** What the app shows instead of a page the host or the dev server could not give. */
    private fun notice(status:Int,title:String,detail:String):WebResourceResponse {
        val html = "<!doctype html><meta name=viewport content='width=device-width,initial-scale=1'><body style='font:16px sans-serif;padding:24px;color:#444'><h2>$title</h2><p>$detail</p>"
        return WebResourceResponse("text/html","utf-8",status,reasonFor(status),mapOf("Cache-Control" to "no-store"),ByteArrayInputStream(html.toByteArray()))
    }
    private fun reasonFor(status:Int) = when(status) {403->"Forbidden";502->"Bad Gateway";504->"Gateway Timeout";else->"Error"}

    /** One request from the page. Returns null to let the WebView do nothing special (never used for the fake origin). */
    fun fetch(url:android.net.Uri,accept:String?,acceptLanguage:String?,method:String):WebResourceResponse {
        if (url.host != PREVIEW_HOST || url.scheme != "https") return WebResourceResponse("text/plain","utf-8",403,"Forbidden",emptyMap(),ByteArrayInputStream(ByteArray(0)))
        if (method != "GET" && method != "HEAD") return notice(405,"Not available in the preview","The preview shows pages only. It cannot send forms or other changes to the dev server.")
        var path = url.encodedPath?.trimStart('/') ?: ""
        val query = url.encodedQuery?.let {"?$it"} ?: ""
        var hops = 0
        while (true) {
            val target = validateAddress(address).newBuilder().encodedPath("/v1/preview/$port/$path").build().toString() + query.takeIf {hops == 0}.orEmpty()
            val builder = Request.Builder().url(target).header("Authorization","Bearer $token").method(method,null)
            accept?.let {builder.header("Accept",it)};acceptLanguage?.let {builder.header("Accept-Language",it)}
            val response = try {client.newCall(builder.build()).execute()} catch(failure:java.io.IOException) {
                return notice(502,"The host could not be reached","Check the connection to your computer and try again.")
            }
            val code = response.code
            val type = response.header("Content-Type")
            if (code in listOf(301,302,303,307,308) && hops < 5) {
                val location = response.header("Location");response.close()
                if (location != null && location.startsWith("/")) {path = location.trimStart('/');hops++;continue}
                return notice(502,"Redirected away","The page tried to go to $location, which the preview does not follow.")
            }
            val body = response.body?.bytes() ?: ByteArray(0)
            response.close()
            if (code == 403 && type?.startsWith("application/json") == true) return when {
                body.decodeToString().contains("preview_disabled") -> notice(403,"Preview is off on this host","Turn it on in the host's config.toml ([preview] enabled = true) and restart acpd.")
                body.decodeToString().contains("port_not_allowed") -> notice(403,"This port is not allowed","The host only shows the ports listed under [preview] ports in its config.toml.")
                else -> notice(403,"Not allowed","The host did not allow this page.")
            }
            if (code == 502 && body.decodeToString().contains("nothing_listening")) return notice(502,"Nothing is listening on port $port","Start the dev server on your computer, then reload.")
            if (code == 504 && body.decodeToString().contains("preview_timeout")) return notice(504,"The dev server did not answer in time","It may be busy or stuck. Try reloading.")
            val mime = type?.substringBefore(';')?.trim()?.ifBlank {null} ?: "application/octet-stream"
            val encoding = type?.substringAfter("charset=",""
            )?.trim()?.trim('"')?.ifBlank {null}
            val headers = mutableMapOf("Cache-Control" to "no-store")
            // Everything else about the dev server's headers is left behind, including cookies and policies.
            response.header("Content-Language")?.let {headers["Content-Language"] = it}
            val safe = if (code in 100..599 && code !in 300..399) code else 200
            return WebResourceResponse(mime,encoding,safe,if (safe < 400) "OK" else "Error",headers,ByteArrayInputStream(if (method == "HEAD") ByteArray(0) else body))
        }
    }
}

/** Where a typed address points: a port on the host's computer and a path. Null when it is not one. */
fun parsePreviewAddress(input:String):Pair<Int?,String>? {
    val text = input.trim().removePrefix("http://").removePrefix("https://")
    val match = Regex("^(?:(?:localhost|127\\.0\\.0\\.1|\\[::1]):)?(\\d{2,5})?((?:/[^\\s]*)?)$").matchEntire(text) ?: return null
    val port = match.groupValues[1].takeIf {it.isNotEmpty()}?.toIntOrNull()
    if (port != null && port !in 1..65535) return null
    if (port == null && match.groupValues[2].isEmpty()) return null
    return port to match.groupValues[2].ifEmpty {"/"}
}
