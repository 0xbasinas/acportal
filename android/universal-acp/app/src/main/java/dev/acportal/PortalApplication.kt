package dev.acportal

import android.app.Application
import dev.acportal.data.*
import dev.acportal.storage.*
import dev.acportal.security.CredentialVault
import kotlinx.coroutines.*

open class PortalApplication : Application() {
    lateinit var repository:PortalRepository
        protected set
    override fun onCreate() {
        super.onCreate()
        cacheDir.listFiles()?.filter {it.isFile && it.name.startsWith("received-audio-") && it.name.endsWith(".tmp")}?.forEach {it.delete()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val vault=CredentialVault(this)
        repository=PortalRepository(PortalDatabase.open(this).portal(),HostApi(HostApi.client(),vault),vault,SettingsStore(this),scope)
    }
}
