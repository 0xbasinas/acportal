package dev.acportal.presentation

import android.Manifest
import android.app.UiAutomation
import android.content.Intent
import android.graphics.Rect
import android.os.SystemClock
import android.provider.Settings
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.MainActivity
import dev.acportal.data.PortalRepository
import dev.acportal.protocol.*
import dev.acportal.storage.HostProfile
import dev.acportal.storage.StoredSession
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test
import java.io.File
import java.security.MessageDigest

/** Opt-in native traversal on actual MainActivity with owned Room/preferences/vault. */
class TalkBackMainActivityUiTest {
    @Test fun darkOfflineEmptyState()=check("dark")
    @Test fun lightOfflineEmptyState()=check("light")
    @Test fun darkProductionBackStackFocus()=check("dark",true)
    @Test fun lightProductionBackStackFocus()=check("light",true)

    private fun snapshot(repository:PortalRepository)=runBlocking {
        val digest=MessageDigest.getInstance("SHA-256")
        listOf(repository.hosts.first().sortedBy {it.id},repository.sessions.first().sortedWith(compareBy({it.hostId},{it.id})),repository.uiState.first(),repository.settings.settings.first(),repository.settings.mcpAliases.first().toSortedMap()).forEach {
            digest.update(it.toString().toByteArray());digest.update(0.toByte())
        }
        digest.digest().toList()
    }

    private fun check(mode:String,focusReport:Boolean=false) {
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val args=InstrumentationRegistry.getArguments()
        assumeTrue("Use the owned native TalkBack runner",args.getString("talkback")=="true" && args.getString("hardwareGestures")=="true")
        val context=instrumentation.targetContext
        require(context.packageName=="dev.acportal")
        val application=context.applicationContext as TaskRecoveryFixtureApplication
        val root=File(context.cacheDir,"task-recovery-fixture")
        check(!root.exists() && application.fixtureStore==null) {"Clean the prior owned task fixture first"}
        val production=application.productionRepository
        val before=snapshot(production)
        val automation=instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
        val resolver=context.contentResolver
        val keys=listOf(Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,Settings.Secure.ACCESSIBILITY_ENABLED,Settings.Secure.TOUCH_EXPLORATION_ENABLED)
        val previous=keys.associateWith {Settings.Secure.getString(resolver,it)}
        fun settings(values:Map<String,String?>) {
            automation.adoptShellPermissionIdentity(Manifest.permission.WRITE_SECURE_SETTINGS)
            try {values.forEach {(key,value)->check(Settings.Secure.putString(resolver,key,value))}}
            finally {automation.dropShellPermissionIdentity()}
        }
        var scenario:ActivityScenario<MainActivity>?=null
        try {
            check(root.mkdirs());File(root,"enabled").writeText("");File(root,"main-enabled").writeText("")
            application.activateOwnedMainFixture()
            val store=application.fixtureStore!!
            val info=SessionInfo("native-offline","fixture-agent","owned-acp","/fixture/native-offline",status="interrupted")
            runBlocking {
                store.settings.theme(mode)
                store.db.portal().saveHost(HostProfile("native-host","Owned workstation","http://127.0.0.1:1","unused","owned-device",0))
                store.db.portal().saveSession(StoredSession("native-host",info.id,WireJson.encodeToString(info),draft="Owned untouched draft"))
                store.db.portal().saveMainPage("sessions")
            }
            val service="com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService"
            val services=(previous[Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES].orEmpty().split(':').filter {it.isNotBlank()}+service).distinct().joinToString(":")
            settings(mapOf(Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES to services,Settings.Secure.ACCESSIBILITY_ENABLED to "1"))
            val manager=context.getSystemService(AccessibilityManager::class.java)
            waitFor("TalkBack connected") {manager.isTouchExplorationEnabled && manager.getEnabledAccessibilityServiceList(-1).any {it.resolveInfo.serviceInfo.packageName=="com.google.android.marvin.talkback"}}
            scenario=ActivityScenario.launch(Intent(context,MainActivity::class.java))
            var dismissedNotification=false
            waitFor("Owned Sessions page") {
                if(!dismissedNotification && automation.rootInActiveWindow?.packageName?.toString()?.endsWith(".permissioncontroller")==true && find(automation,"Allow Android Accessibility Suite to send you notifications?")!=null) {
                    assertTrue(automation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK));dismissedNotification=true
                }
                find(automation,"Sessions")!=null && nodes(automation).any {it.isClickable && label(it).contains("fixture-agent")}
            }
            val row=nodes(automation).first {it.isClickable && label(it).contains("fixture-agent")}
            assertTrue(row.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Interrupted session") {find(automation,"View saved conversation")!=null}
            assertTrue(nodes(automation).first {it.isClickable && label(it)=="View saved conversation"}.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Saved conversation") {find(automation,"New conversation")!=null}
            val heading=find(automation,"New conversation")!!
            assertTrue(heading.isHeading && (heading.isAccessibilityFocused || heading.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS)))
            // Geometry only, from the isolated page, to distinguish clipping from insets.
            instrumentation.sendStatus(2,android.os.Bundle().apply {
                putString("acportalOwnedScrollBounds",nodes(automation).filter {it.isScrollable}.map {node->Rect().also(node::getBoundsInScreen).toShortString()}.joinToString(";"))
            })
            traverseTo(automation) {it=="Session actions"}
            traverseTo(automation) {it=="No messages saved"}
            traverseTo(automation) {it=="Connect to view this conversation. Message caching can be changed in Settings."}
            assertEquals("Owned untouched draft",runBlocking {store.repository.sessions.first().single().draft})
            assertTrue("Reading history must not start a live session",store.repository.liveActivities.value.isEmpty())
            // Back is explicit, requires no host mutation and returns to the interrupted route.
            assertTrue(nodes(automation).first {it.isClickable && label(it)=="Back"}.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            waitFor("Interrupted route restored") {find(automation,"Session interrupted")!=null}
            if(focusReport) {
                // Production back stack: report where native focus lands after each Back; never assume a target.
                fun reported(step:String) {
                    waitFor("Focus after $step") {nodes(automation).any {it.isAccessibilityFocused}}
                    instrumentation.sendStatus(2,android.os.Bundle().apply {putString("acportalFocusAfter$step",label(nodes(automation).first {it.isAccessibilityFocused}))})
                }
                reported("SavedBack")
                assertTrue(automation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK))
                waitFor("Sessions list restored") {nodes(automation).any {it.isClickable && label(it).contains("fixture-agent")}}
                reported("InterruptedBack")
                assertEquals("Back must not start a session or change the draft","Owned untouched draft",runBlocking {store.repository.sessions.first().single().draft})
                assertTrue(store.repository.liveActivities.value.isEmpty())
                // Settings routes on the production stack: open each row, press Back, report where focus lands.
                waitFor("Settings tab") {nodes(automation).any {it.isClickable && label(it).contains("Settings")}}
                assertTrue(nodes(automation).first {it.isClickable && label(it).contains("Settings")}.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Settings page") {nodes(automation).any {it.isHeading && label(it)=="Settings"}}
                for(row in listOf("Connections","MCP servers","Workspace access","Agents")) {
                    waitFor("$row row") {nodes(automation).any {it.isClickable && label(it).startsWith(row)}}
                    // The bottom-navigation tab shares the "Connections" label; the Settings row is the match highest on screen.
                    assertTrue(nodes(automation).filter {it.isClickable && label(it).startsWith(row)}.minBy {Rect().also(it::getBoundsInScreen).top}.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                    waitFor("$row opened") {nodes(automation).none {it.isHeading && label(it)=="Settings"}}
                    assertTrue(automation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK))
                    waitFor("Settings restored after $row") {nodes(automation).any {it.isHeading && label(it)=="Settings"}}
                    reported("Settings${row.filter {it.isLetter()}}Back")
                }
                assertEquals("Settings routes start no session or draft change","Owned untouched draft",runBlocking {store.repository.sessions.first().single().draft})
                assertTrue(store.repository.liveActivities.value.isEmpty())
            }
        } finally {
            try {
                try {scenario?.onActivity {it.finishAndRemoveTask()}}
                finally {scenario?.close()}
            } finally {
                try {application.closeFixtureStore();assertSame(production,application.repository);assertEquals("Production data/preferences changed",before,snapshot(production));check(root.deleteRecursively())}
                finally {settings(previous);waitFor("Accessibility restored") {keys.all {Settings.Secure.getString(resolver,it)==previous[it]}}}
            }
        }
    }

    private fun nodes(automation:UiAutomation):List<AccessibilityNodeInfo> {
        val queue=java.util.ArrayDeque<AccessibilityNodeInfo>();automation.rootInActiveWindow?.let(queue::add)
        val result=mutableListOf<AccessibilityNodeInfo>()
        while(queue.isNotEmpty() && result.size<512) {val node=queue.removeFirst();result.add(node);for(index in 0 until node.childCount)node.getChild(index)?.let(queue::add)}
        return result
    }
    private fun label(node:AccessibilityNodeInfo?):String {
        if(node==null)return ""
        val queue=java.util.ArrayDeque<AccessibilityNodeInfo>().apply {add(node)}
        val labels=linkedSetOf<String>();var count=0
        while(queue.isNotEmpty() && count++<64) {
            val current=queue.removeFirst();listOfNotNull(current.text,current.contentDescription).forEach {labels.add(it.toString())}
            for(index in 0 until current.childCount)current.getChild(index)?.let(queue::add)
        }
        return labels.joinToString(" ")
    }
    private fun find(automation:UiAutomation,text:String)=nodes(automation).firstOrNull {it.text?.toString()==text || it.contentDescription?.toString()==text}
    private fun traverseTo(automation:UiAutomation,predicate:(String)->Boolean) {
        val observed=linkedSetOf<String>()
        repeat(32) {
            val focused=nodes(automation).firstOrNull {it.isAccessibilityFocused}
            val current=label(focused);observed.add(current)
            if(predicate(current))return
            var changed=false
            repeat(3) {attempt->if(!changed) {
                InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {putString("acportalGesture","swipeRight");putString("acportalGestureAttempt",(attempt+1).toString())})
                val deadline=SystemClock.uptimeMillis()+5000
                while(!changed && SystemClock.uptimeMillis()<deadline) {SystemClock.sleep(100);changed=nodes(automation).any {it.isAccessibilityFocused && it!=focused}}
            }}
            assertTrue("Native hardware gesture changed focus",changed)
        }
        fail("Owned MainActivity traversal missed target: $observed")
    }
    private fun waitFor(message:String,condition:()->Boolean) {
        val deadline=SystemClock.uptimeMillis()+15000
        while(!condition() && SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100)
        assertTrue(message,condition())
    }
}
