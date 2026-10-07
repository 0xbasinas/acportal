package dev.acportal.data

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.AudioFocusRequest
import android.media.MediaPlayer
import android.net.Uri
import dev.acportal.protocol.*
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.serialization.json.JsonObject
import java.io.File
import java.util.UUID

suspend fun receivedImagePreview(block:JsonObject):Bitmap=withContext(Dispatchers.Default) {
    require(receivedContentMime(block).startsWith("image/")) {"This content has no supported image type."}
    val bytes=receivedContentBytes(block)
    val bounds=BitmapFactory.Options().apply {inJustDecodeBounds=true}
    BitmapFactory.decodeByteArray(bytes,0,bytes.size,bounds)
    require(bounds.outWidth>0 && bounds.outHeight>0 && bounds.outWidth.toLong()*bounds.outHeight<=64L*1024*1024) {"This image cannot be previewed safely. Save it to inspect it elsewhere."}
    var sample=1
    while(bounds.outWidth/sample>1024 || bounds.outHeight/sample>1024)sample*=2
    BitmapFactory.decodeByteArray(bytes,0,bytes.size,BitmapFactory.Options().apply {inSampleSize=sample}) ?: error("This image format could not be decoded. Save it to inspect it elsewhere.")
}
suspend fun saveReceivedContent(context:Context,uri:Uri,block:JsonObject)=withContext(Dispatchers.IO) {
    val bytes=receivedContentBytes(block)
    context.contentResolver.openOutputStream(uri,"wt")?.use {it.write(bytes)} ?: error("The selected document could not be opened.")
}
enum class AudioPlaybackState { STOPPED, LOADING, PLAYING, FAILED }

/** Embedded bytes only. No URI passed by an agent is opened by the media player. */
class ReceivedAudioPlayer(private val context:Context) {
    val state=MutableStateFlow(AudioPlaybackState.STOPPED)
    private var player:MediaPlayer?=null
    private var file:File?=null
    private var generation=0L
    private val audioManager=context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var focus:AudioFocusRequest?=null
    suspend fun play(block:JsonObject) {
        stop();val version=generation;state.value=AudioPlaybackState.LOADING
        val destination=File(context.cacheDir,"received-audio-${UUID.randomUUID()}.tmp")
        var handedOff=false
        try {
            withContext(Dispatchers.IO) {
                require(receivedContentMime(block).startsWith("audio/"))
                destination.writeBytes(receivedContentBytes(block))
            }
            if(version!=generation)return
            val current=MediaPlayer()
            player=current;file=destination
            current.setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build())
            current.setOnPreparedListener {if(player===it && version==generation) {
                try {
                    val request=AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT).setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_MEDIA).setContentType(AudioAttributes.CONTENT_TYPE_MUSIC).build()).setOnAudioFocusChangeListener {change->if(change<0)stop()}.build()
                    focus=request
                    check(audioManager.requestAudioFocus(request)==AudioManager.AUDIOFOCUS_REQUEST_GRANTED)
                    it.start();state.value=AudioPlaybackState.PLAYING
                } catch(_:Exception) {stop();state.value=AudioPlaybackState.FAILED}
            }}
            current.setOnCompletionListener {if(player===it)stop()}
            current.setOnErrorListener {failed,_,_->if(player===failed) {stop();state.value=AudioPlaybackState.FAILED};true}
            current.setDataSource(destination.absolutePath);current.prepareAsync();handedOff=true
        } catch(cancelled:CancellationException) {if(version==generation)stop();throw cancelled}
        catch(_:Exception) {if(version==generation) {stop();state.value=AudioPlaybackState.FAILED}}
        finally {if(!handedOff)destination.delete()}
    }
    fun stop() {
        generation++;player?.release();player=null;file?.delete();file=null
        focus?.let(audioManager::abandonAudioFocusRequest);focus=null;state.value=AudioPlaybackState.STOPPED
    }
}
