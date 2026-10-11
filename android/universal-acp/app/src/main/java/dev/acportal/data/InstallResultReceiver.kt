package dev.acportal.data

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.os.Build
import dev.acportal.PortalApplication

/** Receives Android's answer to an install session: show its confirmation, or report the failure. */
class InstallResultReceiver:BroadcastReceiver() {
    override fun onReceive(context:Context,intent:Intent) {
        val status=intent.getIntExtra(PackageInstaller.EXTRA_STATUS,PackageInstaller.STATUS_FAILURE)
        if(status==PackageInstaller.STATUS_PENDING_USER_ACTION) {
            val confirm=if(Build.VERSION.SDK_INT>=33)intent.getParcelableExtra(Intent.EXTRA_INTENT,Intent::class.java) else @Suppress("DEPRECATION") intent.getParcelableExtra(Intent.EXTRA_INTENT)
            confirm?.let {context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))}
            return
        }
        (context.applicationContext as? PortalApplication)?.updates?.installFinished(status,intent.getStringExtra(PackageInstaller.EXTRA_STATUS_MESSAGE))
    }
}
