package dev.acportal.data

import android.net.Uri
import androidx.test.platform.app.InstrumentationRegistry
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Test
import java.io.File

class TranscriptExportTest {
    @Test fun documentWriterClosesAndPreservesUtf8WithoutStoragePermissions() = runBlocking {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val file=File.createTempFile("transcript-",".md",context.cacheDir)
        try {
            val transcript="# Conversation\n\nUnicode ✓ 界\n\n```\nliteral code\n```\n"
            saveTranscript(context,Uri.fromFile(file),transcript)
            assertEquals(transcript,file.readText(Charsets.UTF_8))
            saveTranscript(context,Uri.fromFile(file),"short")
            assertEquals("short",file.readText())
        } finally {file.delete()}
    }
}
