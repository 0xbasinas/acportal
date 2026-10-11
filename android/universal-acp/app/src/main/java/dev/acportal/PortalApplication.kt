package dev.acportal

import android.app.Application
import dev.acportal.data.*
import dev.acportal.storage.*
import dev.acportal.security.CredentialVault
import kotlinx.coroutines.*

open class PortalApplication : Application() {
    lateinit var repository:PortalRepository
        protected set
    lateinit var updates:UpdateController
        protected set
    override fun onCreate() {
        super.onCreate()
        cacheDir.listFiles()?.filter {it.isFile && it.name.startsWith("received-audio-") && it.name.endsWith(".tmp")}?.forEach {it.delete()}
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val vault=CredentialVault(this)
        // Follows redirects (GitHub serves release files from another host); the updater checks the final address.
        val updateClient=okhttp3.OkHttpClient.Builder().connectTimeout(10,java.util.concurrent.TimeUnit.SECONDS).readTimeout(30,java.util.concurrent.TimeUnit.SECONDS).followRedirects(true).followSslRedirects(false).build()
        updates=UpdateController(this,AppUpdater(updateClient,cacheDir),scope)
        repository=PortalRepository(PortalDatabase.open(this).portal(),HostApi(HostApi.client(),vault),vault,SettingsStore(this),scope)
    }
}
