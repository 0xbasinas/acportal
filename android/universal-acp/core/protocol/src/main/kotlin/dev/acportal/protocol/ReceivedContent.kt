package dev.acportal.protocol

import java.net.URI
import java.util.Base64
import kotlinx.serialization.json.*

const val MAX_RECEIVED_CONTENT_BYTES=768*1024

fun contentResource(block:JsonObject):JsonObject = if(block["type"].text()=="resource")block["resource"].objectValue() else block
fun contentName(block:JsonObject):String {
    val resource=contentResource(block)
    val uriName=runCatching {URI(resource["uri"].text()).path?.substringAfterLast('/')}.getOrNull().orEmpty()
    val name=block["title"].text().ifBlank {block["name"].text()}.ifBlank {block["_meta"].objectValue()["acportalAttachmentName"].text()}.ifBlank {uriName}
    return name.filterNot(Char::isISOControl).take(256).ifBlank {when(block["type"].text()) {"image"->"Image";"audio"->"Audio";"resource","resource_link"->"Resource";else->"Unsupported content"}}
}
fun receivedContentBytes(block:JsonObject):ByteArray {
    val resource=contentResource(block)
    if(block["type"].text()=="resource" && resource["text"] is JsonPrimitive) {
        val text=resource["text"].text()
        require(text.length<=MAX_RECEIVED_CONTENT_BYTES) {"This resource exceeds the save limit."}
        return text.toByteArray(Charsets.UTF_8).also {require(it.size<=MAX_RECEIVED_CONTENT_BYTES) {"This resource exceeds the save limit."}}
    }
    val data=(if(block["type"].text()=="resource")resource["blob"] else block["data"]).text()
    require(data.length<=MAX_RECEIVED_CONTENT_BYTES/3*4+4) {"This content exceeds the media limit."}
    require(data.isNotBlank()) {"No embedded media data is available."}
    return try {Base64.getDecoder().decode(data).also {require(it.size<=MAX_RECEIVED_CONTENT_BYTES) {"This content exceeds the media limit."}}}
    catch(_:IllegalArgumentException) {throw IllegalArgumentException("This content contains invalid or oversized media data.")}
}
fun receivedContentMime(block:JsonObject):String=contentResource(block)["mimeType"].text().takeIf {it.length<=128 && Regex("[a-zA-Z0-9!#$&^_.+-]+/[a-zA-Z0-9!#$&^_.+-]+").matches(it)} ?: "application/octet-stream"
fun receivedContentFilename(block:JsonObject):String {
    val mime=receivedContentMime(block)
    val name=contentName(block).replace(Regex("[\\\\/:*?\"<>|]"),"_").trim().trim('.')
    val suffix=when(mime) {"image/png"->"png";"image/jpeg"->"jpg";"image/webp"->"webp";"audio/wav","audio/x-wav"->"wav";"audio/mpeg","audio/mp3"->"mp3";"audio/ogg"->"ogg";"text/plain"->"txt";else->"bin"}
    val safe=name.ifBlank {"content"}
    return if('.' in safe)safe else "$safe.$suffix"
}
fun contentWebUrl(block:JsonObject):String? {
    val raw=contentResource(block)["uri"].text()
    val uri=runCatching {URI(raw)}.getOrNull() ?: return null
    return raw.takeIf {uri.scheme?.lowercase() in listOf("https","http") && !uri.host.isNullOrBlank() && uri.userInfo==null && raw.length<=4096 && raw.none(Char::isISOControl)}
}
