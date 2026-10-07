package dev.acportal.data

import android.content.ContentValues
import android.graphics.Bitmap
import android.provider.MediaStore
import android.util.Base64
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test
import java.io.ByteArrayOutputStream

class ReceivedMediaTest {
    @Test fun previewSamplesLargeImagesAndRejectsInvalidData()=runBlocking {
        val bytes=ByteArrayOutputStream().use {out->val bitmap=Bitmap.createBitmap(2048,1024,Bitmap.Config.ARGB_8888);bitmap.compress(Bitmap.CompressFormat.PNG,100,out);bitmap.recycle();out.toByteArray()}
        val block=buildJsonObject {put("type","image");put("mimeType","image/png");put("data",Base64.encodeToString(bytes,Base64.NO_WRAP))}
        val preview=receivedImagePreview(block)
        assertTrue(preview.width<=1024);assertTrue(preview.height<=1024);preview.recycle()
        try {receivedImagePreview(buildJsonObject {put("type","image");put("mimeType","image/png");put("data","AP8=")});fail("Invalid image must be rejected")} catch(_:IllegalArgumentException) {}
        Unit
    }
    @Test fun embeddedBinarySaveWritesExactBytesToChosenDestination()=runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val uri=context.contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI,ContentValues().apply {put(MediaStore.MediaColumns.DISPLAY_NAME,"acportal-received-content-test.bin");put(MediaStore.MediaColumns.MIME_TYPE,"application/octet-stream")})!!
        try {
            saveReceivedContent(context,uri,WireJson.parseToJsonElement("""{"type":"resource","resource":{"uri":"file:///host/a.bin","blob":"AP8BAg=="}}""").objectValue())
            assertArrayEquals(byteArrayOf(0,-1,1,2),context.contentResolver.openInputStream(uri)!!.use {it.readBytes()})
        } finally {context.contentResolver.delete(uri,null,null)}
        Unit
    }
}
