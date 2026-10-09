package dev.acportal.protocol

import java.net.URI
import kotlinx.serialization.json.*

data class PromptAttachment(val name:String,val content:JsonObject)
const val MAX_ATTACHMENTS=4
const val MAX_ATTACHMENT_BYTES=256*1024
const val MAX_ATTACHMENT_WIRE_BYTES=384*1024
const val MAX_PROMPT_WIRE_BYTES=512*1024

fun checkedAttachments(info:SessionInfo,attachments:List<PromptAttachment>):List<PromptAttachment> {
    require(attachments.size<=MAX_ATTACHMENTS) {"Attach up to four files or references."}
    attachments.forEach {attachment->
        require(attachment.name.isNotBlank() && attachment.name.length<=256 && attachment.name.none {it.isISOControl()}) {"The attachment name is invalid."}
        val required=when(attachment.content["type"].text()) {"image"->"image";"audio"->"audio";"resource"->"embeddedContext";"resource_link"->null;else->error("Unsupported attachment type.")}
        require(required==null || info.promptCapability(required)) {"This agent does not support that attachment type."}
    }
    boundedJsonBytes(JsonArray(attachments.map {it.content}),MAX_ATTACHMENT_WIRE_BYTES,"Attachments are too large. Remove a file or choose a smaller one.")
    return attachments
}

fun contextReference(uri:String):PromptAttachment {
    val parsed=try {URI(uri.trim())} catch(_:Exception) {throw IllegalArgumentException("Enter a valid absolute context URI.")}
    require(parsed.isAbsolute && parsed.scheme.lowercase() !in setOf("content","android.resource") && parsed.userInfo==null) {"Use a URI accessible to the agent. Select phone files with Attach file."}
    val name=parsed.path?.substringAfterLast('/')?.takeIf {it.isNotBlank()}?.take(256) ?: "Context reference"
    return PromptAttachment(name,buildJsonObject {put("type","resource_link");put("uri",parsed.toString());put("name",name)})
}

fun promptContent(text:String,attachments:List<PromptAttachment>):JsonArray=buildJsonArray {
    if(text.isNotBlank())add(buildJsonObject {put("type","text");put("text",text)})
    attachments.forEach {add(it.content)}
}

fun promptDisplay(content:JsonArray):String=content.joinToString("\n") {entry->
    val block=entry.objectValue()
    when(block["type"].text()) {
        "text"->block["text"].text()
        "resource_link"->block["uri"].text()
        "resource"->"Attached file: ${block["_meta"].objectValue()["acportalAttachmentName"].text().ifBlank {"Context file"}}"
        "image"->"Attached image: ${block["_meta"].objectValue()["acportalAttachmentName"].text().ifBlank {"Image"}}"
        "audio"->"Attached audio: ${block["_meta"].objectValue()["acportalAttachmentName"].text().ifBlank {"Audio"}}"
        else->"Attached context"
    }
}

fun attachmentMetadata(name:String)=buildJsonObject {put("acportalAttachmentName",name)}
