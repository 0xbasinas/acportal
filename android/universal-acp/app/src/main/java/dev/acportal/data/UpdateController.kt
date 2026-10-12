package dev.acportal.data

import android.content.Context
import android.content.Intent
import android.content.pm.PackageInfo
import android.net.Uri
import android.provider.Settings
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** The build running now. */
data class InstalledBuild(val versionName:String,val build:Long) {
    companion object {
        fun of(info:PackageInfo)=InstalledBuild(info.versionName.orEmpty(),if(android.os.Build.VERSION.SDK_INT>=28)info.longVersionCode else @Suppress("DEPRECATION") info.versionCode.toLong())
    }
}

sealed interface UpdateState {
    data object Idle:UpdateState
    data object Checking:UpdateState
    data class UpToDate(val latest:ReleaseInfo):UpdateState
    data class Available(val release:ReleaseInfo):UpdateState
    data class Downloading(val release:ReleaseInfo,val done:Long):UpdateState
    /** Android has to be told once that this app may install apps. */
    data class NeedsPermission(val release:ReleaseInfo):UpdateState
    data class Installing(val release:ReleaseInfo):UpdateState
    /** [release] is kept when there is one, so Try again can resume from it. */
    data class Failed(val message:String,val release:ReleaseInfo?=null):UpdateState
}

/**
 * Owns the update flow for the whole process, so a download survives leaving the screen or a
 * rotation. Nothing here runs unless the user opens the Update app screen and asks for it.
 */
class UpdateController(private val context:Context,private val updater:AppUpdater,private val scope:CoroutineScope) {
    private val mutable=MutableStateFlow<UpdateState>(UpdateState.Idle)
    val state:StateFlow<UpdateState> = mutable.asStateFlow()
    val installed:InstalledBuild = InstalledBuild.of(context.packageManager.getPackageInfo(context.packageName,0))
    private var job:Job?=null
    private val preferences=context.getSharedPreferences("update",Context.MODE_PRIVATE)
    private val mutableChannel=MutableStateFlow(UpdateChannel.fromName(preferences.getString("channel",null)) ?: UpdateChannel.installed(installed.versionName))
    /** Which kind of build this screen follows; remembered between visits. Defaults to the kind that is installed. */
    val channel:StateFlow<UpdateChannel> = mutableChannel.asStateFlow()

    /** Switches channel and looks again. Ignored while a download or install is under way. */
    fun selectChannel(value:UpdateChannel) {
        val current=mutable.value
        if(value==mutableChannel.value || current is UpdateState.Downloading || current is UpdateState.Installing)return
        job?.cancel();job=null
        mutableChannel.value=value
        preferences.edit().putString("channel",value.name).apply()
        check()
    }

    fun check() {
        if(job?.isActive==true)return
        mutable.value=UpdateState.Checking
        val channel=mutableChannel.value
        job=scope.launch {
            mutable.value=try {
                val release=updater.latest(channel)
                if(isUpdateFor(installed,channel,release))UpdateState.Available(release) else UpdateState.UpToDate(release)
            } catch(cancelled:CancellationException) {throw cancelled}
            catch(failure:Exception) {UpdateState.Failed(failure.message ?: "The update check failed.")}
        }
    }
    fun canInstall()=context.packageManager.canRequestPackageInstalls()
    /** Where the user turns on "install unknown apps" for this app. */
    fun permissionIntent():Intent=Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,Uri.parse("package:${context.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    /** Back from the system settings page: offer the install again once the permission is on. */
    fun permissionMayHaveChanged() {
        val current=mutable.value
        if(current is UpdateState.NeedsPermission && canInstall())mutable.value=UpdateState.Available(current.release)
    }
    fun install(release:ReleaseInfo) {
        if(job?.isActive==true)return
        if(!canInstall()) {mutable.value=UpdateState.NeedsPermission(release);return}
        mutable.value=UpdateState.Downloading(release,0)
        job=scope.launch {
            try {
                val file=updater.download(release) {done->mutable.update {if(it is UpdateState.Downloading)it.copy(done=done) else it}}
                mutable.value=UpdateState.Installing(release)
                ApkInstaller.install(context,file)
            } catch(cancelled:CancellationException) {mutable.value=UpdateState.Available(release);throw cancelled}
            catch(failure:Exception) {mutable.value=UpdateState.Failed(failure.message ?: "The update failed.",release)}
        }
    }
    fun cancel() {job?.cancel()}
    /** Called by the install receiver with Android's verdict. */
    fun installFinished(status:Int,message:String?) {
        val release=(mutable.value as? UpdateState.Installing)?.release
        if(status==android.content.pm.PackageInstaller.STATUS_SUCCESS) {updater.clear();return}
        mutable.value=UpdateState.Failed(ApkInstaller.explain(status,message),release)
    }
}
