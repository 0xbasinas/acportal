package dev.acportal.data

import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.protocol.*
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test

class AttachmentLoaderTest {
    private val context=InstrumentationRegistry.getInstrumentation().targetContext
    private fun uri(name:String)=Uri.parse("content://dev.acportal.test.attachments/$name")
    @Test fun textAndBinaryResourcesEmbedBytesWithoutLeakingPhoneUri()=runBlocking {
        val text=loadAttachment(context,uri("text"),AttachmentKind.FILE)
        assertEquals("fixture.kt",text.name)
        assertEquals("val greeting = \"Hello, 世界\"",text.content["resource"].objectValue()["text"].text())
        assertFalse(text.content.toString().contains("content://"))
        val binary=loadAttachment(context,uri("binary"),AttachmentKind.FILE)
        assertEquals("AP8BAg==",binary.content["resource"].objectValue()["blob"].text())
        Unit
    }
    @Test fun mediaUsesBase64AndDeclaredMimeType()=runBlocking {
        val image=loadAttachment(context,uri("image"),AttachmentKind.IMAGE)
        assertEquals("image/png",image.content["mimeType"].text())
        assertTrue(image.content["data"].text().startsWith("iVBOR"))
        val audio=loadAttachment(context,uri("audio"),AttachmentKind.AUDIO)
        assertEquals("audio/wav",audio.content["mimeType"].text())
        assertFalse(audio.content["data"].text().contains('\n'))
        Unit
    }
    @Test fun oversizedFilesAndWrongMediaTypesAreRejected()=runBlocking {
        for((document,kind) in listOf("large" to AttachmentKind.FILE,"text" to AttachmentKind.IMAGE)) {
            try {loadAttachment(context,uri(document),kind);fail("Expected rejected attachment")} catch(_:IllegalArgumentException) {}
        }
        Unit
    }
}
