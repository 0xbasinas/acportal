package dev.acportal.data

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
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
    val image=kind==AttachmentKind.IMAGE || mime.startsWith("image/")
    val readLimit=if(image) 8*1024*1024 else MAX_ATTACHMENT_BYTES
    val bytes=resolver.openInputStream(uri)?.use {input->
        val output=ByteArrayOutputStream();val buffer=ByteArray(8192)
        while(output.size()<=readLimit) {
            val count=input.read(buffer,0,minOf(buffer.size,readLimit+1-output.size()))
            if(count<0)break
            if(count>0)output.write(buffer,0,count)
        }
        output.toByteArray()
    } ?: error("The selected file could not be read.")
    require(bytes.size<=readLimit) {if(image) "Choose an image smaller than 8 MB." else "Choose a file smaller than 256 KiB."}
    // A photo from the camera is larger than one prompt can carry, so it is scaled to a jpeg that fits.
    val shrunk=image && bytes.size>MAX_ATTACHMENT_BYTES
    val payload=if(shrunk) jpegUnderLimit(bytes) else bytes
    val payloadMime=if(shrunk) "image/jpeg" else mime
    val sendKind=if(shrunk) AttachmentKind.IMAGE else kind
    val content=buildJsonObject {
        put("_meta",attachmentMetadata(name))
        when(sendKind) {
            AttachmentKind.IMAGE,AttachmentKind.AUDIO->{
                val type=if(sendKind==AttachmentKind.IMAGE)"image" else "audio"
                require(payloadMime.startsWith("$type/")) {"The selected file has the wrong media type."}
                put("type",type);put("mimeType",payloadMime);put("data",Base64.encodeToString(payload,Base64.NO_WRAP))
            }
            AttachmentKind.FILE->{
                put("type","resource")
                putJsonObject("resource") {
                    put("uri","acportal-attachment://${UUID.randomUUID()}/${Uri.encode(name)}");put("mimeType",payloadMime)
                    val text=if(payload.any {it==0.toByte()})null else runCatching {Charsets.UTF_8.newDecoder().onMalformedInput(CodingErrorAction.REPORT).onUnmappableCharacter(CodingErrorAction.REPORT).decode(ByteBuffer.wrap(payload)).toString()}.getOrNull()
                    if(text!=null)put("text",text) else put("blob",Base64.encodeToString(payload,Base64.NO_WRAP))
                }
            }
        }
    }
    PromptAttachment(name,content)
}

/** A jpeg of `bytes` at most [MAX_ATTACHMENT_BYTES], scaled down until it fits. */
private fun jpegUnderLimit(bytes:ByteArray):ByteArray {
    val bounds=BitmapFactory.Options().apply {inJustDecodeBounds=true}
    BitmapFactory.decodeByteArray(bytes,0,bytes.size,bounds)
    val width=bounds.outWidth; val height=bounds.outHeight
    if(width<=0 || height<=0) error("This image could not be read.")
    var sample=1
    while(width/sample>2048 || height/sample>2048) sample*=2
    var bitmap=BitmapFactory.decodeByteArray(bytes,0,bytes.size,BitmapFactory.Options().apply {inSampleSize=sample}) ?: error("This image could not be read.")
    try {
        var quality=82
        while(true) {
            val out=ByteArrayOutputStream()
            if(!bitmap.compress(Bitmap.CompressFormat.JPEG,quality,out)) error("This image could not be read.")
            if(out.size()<=MAX_ATTACHMENT_BYTES) return out.toByteArray()
            if(quality>45) {quality-=12; continue}
            if(bitmap.width<=480 || bitmap.height<=480) error("Choose a file smaller than 256 KiB.")
            val next=Bitmap.createScaledBitmap(bitmap,bitmap.width/2,bitmap.height/2,true)
            bitmap.recycle(); bitmap=next; quality=75
        }
    } finally { if(!bitmap.isRecycled) bitmap.recycle() }
}
