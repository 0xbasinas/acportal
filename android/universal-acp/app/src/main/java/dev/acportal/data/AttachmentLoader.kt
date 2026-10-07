package dev.acportal.data

import android.content.Context
import android.net.Uri
import android.provider.OpenableColumns
import android.util.Base64
import dev.acportal.protocol.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.*
import java.nio.ByteBuffer
import java.nio.charset.CodingErrorAction
import java.util.UUID
import java.io.ByteArrayOutputStream

enum class AttachmentKind { FILE, IMAGE, AUDIO }

/** Reads only the document chosen by the user. Nothing is sent until prompt submission. */
suspend fun loadAttachment(context:Context,uri:Uri,kind:AttachmentKind):PromptAttachment=withContext(Dispatchers.IO) {
    val resolver=context.contentResolver
    val name=resolver.query(uri,arrayOf(OpenableColumns.DISPLAY_NAME),null,null,null)?.use {cursor->
        if(cursor.moveToFirst())cursor.getString(0) else null
    }?.filterNot {it.isISOControl()}?.take(256)?.takeIf {it.isNotBlank()} ?: "Selected file"
    val mime=resolver.getType(uri)?.takeIf {it.length<=128 && it.none(Char::isISOControl)} ?: "application/octet-stream"
    val bytes=resolver.openInputStream(uri)?.use {input->
        val output=ByteArrayOutputStream();val buffer=ByteArray(8192)
        while(output.size()<=MAX_ATTACHMENT_BYTES) {
            val count=input.read(buffer,0,minOf(buffer.size,MAX_ATTACHMENT_BYTES+1-output.size()))
            if(count<0)break
            if(count>0)output.write(buffer,0,count)
        }
        output.toByteArray()
    } ?: error("The selected file could not be read.")
    require(bytes.size<=MAX_ATTACHMENT_BYTES) {"Choose a file smaller than 256 KiB."}
    val content=buildJsonObject {
        put("_meta",attachmentMetadata(name))
        when(kind) {
            AttachmentKind.IMAGE,AttachmentKind.AUDIO->{
                val type=if(kind==AttachmentKind.IMAGE)"image" else "audio"
                require(mime.startsWith("$type/")) {"The selected file has the wrong media type."}
                put("type",type);put("mimeType",mime);put("data",Base64.encodeToString(bytes,Base64.NO_WRAP))
            }
            AttachmentKind.FILE->{
                put("type","resource")
                putJsonObject("resource") {
                    put("uri","acportal-attachment://${UUID.randomUUID()}/${Uri.encode(name)}");put("mimeType",mime)
                    val text=if(bytes.any {it==0.toByte()})null else runCatching {Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(bytes)).toString()}.getOrNull()
                    if(text!=null)put("text",text) else put("blob",Base64.encodeToString(bytes,Base64.NO_WRAP))
                }
            }
        }
    }
    PromptAttachment(name,content)
}
