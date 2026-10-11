package dev.acportal.data

import android.content.ContentValues
import android.graphics.Bitmap
import android.net.Uri
import android.provider.MediaStore
import android.util.Base64
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream

/** Uses the platform MediaStore provider (not the fixture provider) and the device image codecs. */
class RealProviderAndCodecTest {
    private val context=InstrumentationRegistry.getInstrumentation().targetContext
    private fun insert(name:String,mime:String,bytes:ByteArray):Uri {
        val uri=context.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {
            put(MediaStore.MediaColumns.DISPLAY_NAME,name);put(MediaStore.MediaColumns.MIME_TYPE,mime)})!!
        context.contentResolver.openOutputStream(uri)!!.use {it.write(bytes)}
        return uri
    }
    private fun encode(format:Bitmap.CompressFormat):ByteArray=ByteArrayOutputStream().use {out->
        val bitmap=Bitmap.createBitmap(64,32,Bitmap.Config.ARGB_8888);bitmap.eraseColor(0xFF2266AA.toInt())
        bitmap.compress(format,90,out);bitmap.recycle();out.toByteArray()
    }
    @Test fun platformProviderTextAndImageLoadWithoutLeakingUris()=runBlocking {
        val text=insert("acportal-real-provider.txt","text/plain","Plain provider text — 世界".toByteArray())
        val png=insert("acportal-real-provider.png","image/png",encode(Bitmap.CompressFormat.PNG))
        try {
            val file=loadAttachment(context,text,AttachmentKind.FILE)
            assertEquals("Plain provider text — 世界",file.content["resource"].objectValue()["text"].text())
            assertFalse(file.content.toString().contains("content://"))
            val image=loadAttachment(context,png,AttachmentKind.IMAGE)
            assertEquals("image/png",image.content["mimeType"].text())
            assertTrue(image.content["data"].text().startsWith("iVBOR"))
            try {loadAttachment(context,text,AttachmentKind.IMAGE);fail("Text is not an image")} catch(_:IllegalArgumentException) {}
        } finally {context.contentResolver.delete(text,null,null);context.contentResolver.delete(png,null,null)}
        Unit
    }
    @Test fun devicePreviewDecodesPngJpegAndWebp()=runBlocking {
        for((mime,format) in listOf("image/png" to Bitmap.CompressFormat.PNG,"image/jpeg" to Bitmap.CompressFormat.JPEG,"image/webp" to Bitmap.CompressFormat.WEBP_LOSSY)) {
            val block=buildJsonObject {put("type","image");put("mimeType",mime);put("data",Base64.encodeToString(encode(format),Base64.NO_WRAP))}
            val preview=receivedImagePreview(block)
            assertEquals(mime,64,preview.width);assertEquals(mime,32,preview.height);preview.recycle()
        }
        Unit
    }
}
