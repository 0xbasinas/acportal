package dev.acportal.data

import android.content.Context
import android.net.Uri
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

suspend fun saveTranscript(context:Context,uri:Uri,text:String) = withContext(Dispatchers.IO) {
    context.contentResolver.openOutputStream(uri,"wt")?.bufferedWriter(Charsets.UTF_8)?.use {it.write(text)}
        ?: error("The selected document could not be opened")
}
