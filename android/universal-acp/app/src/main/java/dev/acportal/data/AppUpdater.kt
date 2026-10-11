package dev.acportal.data

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.app.PendingIntent
import dev.acportal.protocol.WireJson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.coroutines.ensureActive
import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.IOException
import java.security.MessageDigest
import kotlin.coroutines.coroutineContext

/** Where updates come from. Fixed on purpose: the app never installs from any other address. */
object UpdateSource {
    const val REPOSITORY="0xbasinas/acportal"
    /** The static tag the Build APK workflow keeps pointing at the newest debug build. */
    const val TAG="debug"
    const val RELEASE_URL="https://api.github.com/repos/$REPOSITORY/releases/tags/$TAG"
    /** GitHub serves release files from these hosts, after redirects. */
    fun allowedHost(host:String)=host=="github.com" || host=="api.github.com" || host.endsWith(".githubusercontent.com") || host=="github-releases.githubusercontent.com"
}

/** One downloadable build from the release. [build] is the run number, which is also the app's versionCode. */
data class ReleaseInfo(val versionName:String,val build:Long,val assetName:String,val url:String,val sizeBytes:Long,val sha256:String?,val publishedAt:String?)

@Serializable private data class GithubAsset(val name:String="",val size:Long=0,@SerialName("browser_download_url") val url:String="",val digest:String?=null)
@Serializable private data class GithubRelease(val name:String?=null,@SerialName("published_at") val publishedAt:String?=null,val assets:List<GithubAsset> = emptyList())

private val assetBuild=Regex("""acportal-debug-(\d+)-""")
private val versionBuild=Regex("""debug\.(\d+)""")

/** Reads the GitHub release JSON. Throws [IllegalArgumentException] when it holds no usable APK. */
fun parseRelease(json:String):ReleaseInfo {
    val release=try {WireJson.decodeFromString<GithubRelease>(json)} catch(failure:Exception) {throw IllegalArgumentException("The release information could not be read.",failure)}
    val asset=release.assets.firstOrNull {it.name.endsWith(".apk") && it.url.isNotBlank()} ?: throw IllegalArgumentException("The latest release has no app file.")
    val versionName=release.name.orEmpty().removePrefix("Debug build").trim().ifBlank {asset.name.removeSuffix(".apk")}
    val build=assetBuild.find(asset.name)?.groupValues?.get(1)?.toLongOrNull() ?: versionBuild.find(versionName)?.groupValues?.get(1)?.toLongOrNull() ?: throw IllegalArgumentException("The latest release has no build number.")
    val sha=asset.digest?.removePrefix("sha256:")?.lowercase()?.takeIf {it.length==64 && it.all {c->c in '0'..'9' || c in 'a'..'f'}}
    return ReleaseInfo(versionName,build,asset.name,asset.url,asset.size,sha,release.publishedAt)
}

/** An update is offered only for a strictly higher build number. */
fun isUpdate(installedBuild:Long,release:ReleaseInfo)=release.build>installedBuild

/** SHA-256 of a file as lower-case hex. */
fun sha256Hex(file:File):String {
    val digest=MessageDigest.getInstance("SHA-256")
    file.inputStream().use {input->val buffer=ByteArray(64*1024);while(true) {val read=input.read(buffer);if(read<0)break;digest.update(buffer,0,read)}}
    return digest.digest().joinToString("") {"%02x".format(it)}
}

/** Checks the release and downloads its APK into the app's cache, verifying size and checksum. */
/** The only addresses an update may be fetched from: https on GitHub's own hosts. */
fun isTrustedUpdateAddress(url:okhttp3.HttpUrl)=url.scheme=="https" && UpdateSource.allowedHost(url.host)

class AppUpdater(private val client:OkHttpClient,private val cacheDir:File,private val acceptAddress:(okhttp3.HttpUrl)->Boolean=::isTrustedUpdateAddress) {
    suspend fun latest():ReleaseInfo=withContext(Dispatchers.IO) {
        val request=Request.Builder().url(UpdateSource.RELEASE_URL).header("Accept","application/vnd.github+json").build()
        client.newCall(request).execute().use {response->
            if(response.code==404)throw IOException("No release has been published yet.")
            if(!response.isSuccessful)throw IOException("GitHub answered ${response.code}. Try again later.")
            val body=response.body ?: throw IOException("GitHub sent an empty answer.")
            if(body.contentLength()>2_000_000)throw IOException("GitHub sent an unexpectedly large answer.")
            parseRelease(body.string())
        }
    }
    /** Downloads to a fixed cache file (replacing any earlier one) and returns it only if it checks out. */
    suspend fun download(release:ReleaseInfo,onProgress:(Long)->Unit):File=withContext(Dispatchers.IO) {
        val target=File(cacheDir,"updates").apply {mkdirs()}.resolve("update.apk")
        val partial=File(target.parentFile,"update.apk.part")
        partial.delete();target.delete()
        try {
            val request=Request.Builder().url(release.url).build()
            client.newCall(request).execute().use {response->
                if(!response.isSuccessful)throw IOException("The download failed (${response.code}).")
                val final=response.request.url
                if(!acceptAddress(final))throw IOException("The download came from an unexpected address.")
                val body=response.body ?: throw IOException("The download was empty.")
                var done=0L
                partial.outputStream().use {output->
                    body.byteStream().use {input->
                        val buffer=ByteArray(64*1024)
                        while(true) {
                            coroutineContext.ensureActive()
                            val read=input.read(buffer)
                            if(read<0)break
                            output.write(buffer,0,read);done+=read;onProgress(done)
                        }
                    }
                }
            }
            if(release.sizeBytes>0 && partial.length()!=release.sizeBytes)throw IOException("The download was incomplete. Try again.")
            if(release.sha256!=null && sha256Hex(partial)!=release.sha256)throw IOException("The download did not match the release checksum, so it was discarded.")
            if(!partial.renameTo(target))throw IOException("The download could not be saved.")
            target
        } finally {partial.delete()}
    }
    fun clear() {File(cacheDir,"updates").deleteRecursively()}
}

/** Hands a downloaded APK to Android's installer. Android verifies the signature and asks the user to confirm. */
object ApkInstaller {
    const val ACTION="dev.acportal.INSTALL_RESULT"
    fun install(context:Context,apk:File) {
        val installer=context.packageManager.packageInstaller
        val params=PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {setSize(apk.length())}
        val id=installer.createSession(params)
        installer.openSession(id).use {session->
            apk.inputStream().use {input->session.openWrite("update.apk",0,apk.length()).use {output->input.copyTo(output);session.fsync(output)}}
            val intent=Intent(context,InstallResultReceiver::class.java).setAction(ACTION)
            val pending=PendingIntent.getBroadcast(context,id,intent,PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
            session.commit(pending.intentSender)
        }
    }
    /** Plain-language reason for a failed install. */
    fun explain(status:Int,message:String?):String=when(status) {
        PackageInstaller.STATUS_FAILURE_INCOMPATIBLE -> "This build is signed with a different key than the installed app, so Android refused it. Uninstall the app once (saved connections are removed) and install this build."
        PackageInstaller.STATUS_FAILURE_STORAGE -> "There is not enough storage space to install the update."
        PackageInstaller.STATUS_FAILURE_ABORTED -> "The install was cancelled."
        PackageInstaller.STATUS_FAILURE_BLOCKED -> "Android blocked the install on this phone."
        else -> "The install did not finish${message?.takeIf {it.isNotBlank()}?.let {": $it"} ?: "."}"
    }
}
