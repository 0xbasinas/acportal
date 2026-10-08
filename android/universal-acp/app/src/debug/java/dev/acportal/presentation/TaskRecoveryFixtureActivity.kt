package dev.acportal.presentation

import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.os.Bundle
import android.os.Process
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.datastore.preferences.core.PreferenceDataStoreFactory
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import dev.acportal.PortalApplication
import dev.acportal.data.PortalRepository
import dev.acportal.security.CredentialVault
import dev.acportal.storage.PortalDatabase
import dev.acportal.storage.SettingsStore
import kotlinx.coroutines.*
import java.io.File
import java.util.Properties

/** Owned persistent storage lets Android restore this debug task in a new process. */
class TaskRecoveryFixtureStore(context:Context) {
    val root=File(context.cacheDir,"task-recovery-fixture")
    val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
    val db=Room.databaseBuilder<PortalDatabase>(context,File(root,"portal.db").absolutePath).setDriver(AndroidSQLiteDriver()).build()
    val settings=SettingsStore(context,PreferenceDataStoreFactory.create(scope=scope,produceFile={File(root,"settings.preferences_pb")}))
    val vault=CredentialVault(object:ContextWrapper(context) {override fun getNoBackupFilesDir()=root})
    val repository=PortalRepository(db.portal(),(context.applicationContext as PortalApplication).repository.api,vault,settings,scope)
    fun close() {runBlocking {scope.coroutineContext[Job]!!.cancelAndJoin()};db.close()}
}

class TaskRecoveryFixtureActivity:ComponentActivity() {
    private lateinit var store:TaskRecoveryFixtureStore
    override fun onCreate(savedInstanceState:Bundle?) {
        super.onCreate(savedInstanceState)
        val root=File(cacheDir,"task-recovery-fixture")
        if(!File(root,"enabled").exists()) {finish();return}
        store=TaskRecoveryFixtureStore(this)
        Properties().apply {
            setProperty("pid",Process.myPid().toString())
            setProperty("task",taskId.toString())
            setProperty("savedState",(savedInstanceState!=null).toString())
            File(root,"created.properties").outputStream().use {store(it,"Fixture lifecycle identity only")}
        }
        setContent {
            val vm:PortalViewModel=viewModel(factory=viewModelFactory {initializer {PortalViewModel(store.repository)}})
            PortalTheme {PortalNavigation(vm)}
        }
    }
    override fun onSaveInstanceState(outState:Bundle) {
        super.onSaveInstanceState(outState)
        Properties().apply {
            setProperty("pid",Process.myPid().toString());setProperty("task",taskId.toString())
            File(cacheDir,"task-recovery-fixture/saved.properties").outputStream().use {store(it,"Framework saved state completed")}
        }
    }
    override fun onNewIntent(intent:Intent) {
        super.onNewIntent(intent)
        if(intent.getBooleanExtra("finish_fixture",false))finishAndRemoveTask()
    }
    override fun onDestroy() {
        super.onDestroy()
        if(::store.isInitialized)store.close()
    }
}
