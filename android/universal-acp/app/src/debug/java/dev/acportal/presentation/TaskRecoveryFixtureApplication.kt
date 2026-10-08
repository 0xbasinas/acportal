package dev.acportal.presentation

import android.app.Activity
import android.app.Application
import android.os.Bundle
import android.os.Process
import dev.acportal.MainActivity
import dev.acportal.PortalApplication
import dev.acportal.data.PortalRepository
import java.io.File
import java.util.Properties

/** Normal debug behavior unless the owned main-task fixture marker is present. */
class TaskRecoveryFixtureApplication:PortalApplication() {
    lateinit var productionRepository:PortalRepository
        private set
    var fixtureStore:TaskRecoveryFixtureStore?=null
        private set
    private var fixtureCallbacks:Application.ActivityLifecycleCallbacks?=null
    override fun onCreate() {
        super.onCreate()
        productionRepository=repository
        val root=File(cacheDir,"task-recovery-fixture")
        if(!File(root,"main-enabled").exists())return
        val store=TaskRecoveryFixtureStore(this)
        fixtureStore=store;repository=store.repository
        val callbacks=object:Application.ActivityLifecycleCallbacks {
            override fun onActivityCreated(activity:Activity,state:Bundle?) {
                if(activity is MainActivity)Properties().apply {
                    setProperty("pid",Process.myPid().toString());setProperty("task",activity.taskId.toString())
                    setProperty("savedState",(state!=null).toString())
                    File(root,"created.properties").outputStream().use {store(it,"MainActivity fixture lifecycle identity")}
                }
            }
            override fun onActivitySaveInstanceState(activity:Activity,state:Bundle) {
                if(activity is MainActivity)Properties().apply {
                    setProperty("pid",Process.myPid().toString());setProperty("task",activity.taskId.toString())
                    File(root,"saved.properties").outputStream().use {store(it,"MainActivity framework state completed")}
                }
            }
            override fun onActivityStarted(activity:Activity) {}
            override fun onActivityResumed(activity:Activity) {}
            override fun onActivityPaused(activity:Activity) {}
            override fun onActivityStopped(activity:Activity) {}
            override fun onActivityDestroyed(activity:Activity) {}
        }
        fixtureCallbacks=callbacks;registerActivityLifecycleCallbacks(callbacks)
    }
    fun closeFixtureStore() {
        fixtureCallbacks?.let(::unregisterActivityLifecycleCallbacks);fixtureCallbacks=null
        fixtureStore?.close();fixtureStore=null;repository=productionRepository
    }
}
