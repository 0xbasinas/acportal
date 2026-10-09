package dev.acportal.presentation

import android.Manifest
import android.app.UiAutomation
import android.content.Intent
import android.os.SystemClock
import android.provider.Settings
import android.view.accessibility.AccessibilityManager
import android.view.accessibility.AccessibilityNodeInfo
import androidx.test.core.app.ActivityScenario
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Test

/** Explicit opt-in via scripts/verify-talkback-ui.ps1. Restores secure settings in finally. */
class TalkBackShellUiTest {
    @Test fun darkTalkBackTraversesCommandAndRequiresExplicitDecision()=check("dark")
    @Test fun lightTalkBackTraversesCommandAndRequiresExplicitDecision()=check("light")
    @Test fun darkTalkBackLogsFiltersAndLiveSwitch()=check("dark","logs")
    @Test fun lightTalkBackLogsFiltersAndLiveSwitch()=check("light","logs")
    @Test fun darkTalkBackAgentsInspectAndReturn()=check("dark","agents")
    @Test fun lightTalkBackAgentsInspectAndReturn()=check("light","agents")
    @Test fun darkTalkBackRecoveryDismissalKeepsReconnect()=check("dark","recovery")
    @Test fun lightTalkBackRecoveryDismissalKeepsReconnect()=check("light","recovery")
    @Test fun darkTalkBackToolDisclosureKeepsFocus()=check("dark","tools")
    @Test fun lightTalkBackToolDisclosureKeepsFocus()=check("light","tools")
    @Test fun darkTalkBackSavedAgentsCannotLaunch()=check("dark","agents-saved")
    @Test fun lightTalkBackSavedAgentsCannotLaunch()=check("light","agents-saved")
    @Test fun darkTalkBackErrorAgentsCannotLaunch()=check("dark","agents-error")
    @Test fun lightTalkBackErrorAgentsCannotLaunch()=check("light","agents-error")
    @Test fun darkTalkBackRemainingBadgeOutcomes()=check("dark","badges")
    @Test fun lightTalkBackRemainingBadgeOutcomes()=check("light","badges")
    @Test fun darkTalkBackOwnToolsAndWriteNotices()=check("dark","notices")
    @Test fun lightTalkBackOwnToolsAndWriteNotices()=check("light","notices")
    @Test fun darkTalkBackElicitationExplicitCancel()=check("dark","elicitation")
    @Test fun lightTalkBackElicitationExplicitCancel()=check("light","elicitation")
    private fun check(mode:String,page:String="shell") {
        assumeTrue("Run this settings-changing fixture through scripts/verify-talkback-ui.ps1",
            InstrumentationRegistry.getArguments().getString("talkback")=="true" &&
                InstrumentationRegistry.getArguments().getString("hardwareGestures")=="true")
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val context=instrumentation.targetContext
        val automation=instrumentation.getUiAutomation(UiAutomation.FLAG_DONT_SUPPRESS_ACCESSIBILITY_SERVICES)
        automation.serviceInfo=automation.serviceInfo.apply {
            flags=flags or android.accessibilityservice.AccessibilityServiceInfo.FLAG_RETRIEVE_INTERACTIVE_WINDOWS
        }
        val resolver=context.contentResolver
        val keys=listOf(Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES,Settings.Secure.ACCESSIBILITY_ENABLED,Settings.Secure.TOUCH_EXPLORATION_ENABLED)
        val previous=keys.associateWith {Settings.Secure.getString(resolver,it)}
        val service="com.google.android.marvin.talkback/com.google.android.marvin.talkback.TalkBackService"
        fun notificationState():String=automation.executeShellCommand("dumpsys package com.google.android.marvin.talkback").use {
            android.os.ParcelFileDescriptor.AutoCloseInputStream(it).bufferedReader().readLines()
                .first {line->line.contains("android.permission.POST_NOTIFICATIONS: granted=")}.trim()
        }
        val notificationBefore=notificationState()
        var scenario:ActivityScenario<UiAccessibilityFixtureActivity>?=null
        fun settings(values:Map<String,String?>) {
            automation.adoptShellPermissionIdentity(Manifest.permission.WRITE_SECURE_SETTINGS)
            try {values.forEach {(key,value)->assertTrue(Settings.Secure.putString(resolver,key,value))}}
            finally {automation.dropShellPermissionIdentity()}
        }
        try {
            val services=(previous[Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES].orEmpty().split(':').filter {it.isNotBlank()}+service).distinct().joinToString(":")
            settings(mapOf(Settings.Secure.ENABLED_ACCESSIBILITY_SERVICES to services,Settings.Secure.ACCESSIBILITY_ENABLED to "1"))
            val manager=context.getSystemService(AccessibilityManager::class.java)
            waitFor("TalkBack service connected") {
                manager.getEnabledAccessibilityServiceList(-1).any {it.resolveInfo.serviceInfo.packageName=="com.google.android.marvin.talkback"} && manager.isTouchExplorationEnabled
            }
            val title=when(page) {"logs"->"Connection logs";"agents","agents-saved","agents-error"->"Agents";"recovery"->"Recovery fixture";"tools","badges"->"Activity review";"notices"->"Agent safety notices";"elicitation"->"The agent is asking you";else->"Host permission needed"}
            scenario=ActivityScenario.launch(Intent(context,UiAccessibilityFixtureActivity::class.java).putExtra("mode",mode).putExtra("page",page))
            waitFor("Fixture window") {
                val root=automation.rootInActiveWindow
                // First service startup may request notifications. Back cancels without granting
                // or changing its permission flags; only this known TalkBack prompt is dismissed.
                if(root?.packageName?.toString()?.endsWith(".permissioncontroller")==true &&
                    nodes(automation).any {it.text?.toString()=="Allow Android Accessibility Suite to send you notifications?"}) {
                    assertTrue(automation.performGlobalAction(android.accessibilityservice.AccessibilityService.GLOBAL_ACTION_BACK))
                }
                find(automation,title)!=null
            }
            val heading=find(automation,title)!!
            assertTrue("TalkBack touch exploration remains enabled after fixture launch",manager.isTouchExplorationEnabled)
            assertTrue("Page title is a native heading",heading.isHeading)
            assertTrue(heading.isAccessibilityFocused || heading.performAction(AccessibilityNodeInfo.ACTION_ACCESSIBILITY_FOCUS))
            if(page!="shell") {
                checkPage(page,automation,scenario)
                return
            }
            assertTrue("Fixture command missing from native tree: ${nodes(automation).map {it.text?.toString() ?: it.contentDescription?.toString()}}",
                nodes(automation).any {it.text?.contains("review the complete fixture command")==true})
            val observed=linkedSetOf<String>()
            for(step in 0 until 24) {
                val focused=nodes(automation).firstOrNull {it.isAccessibilityFocused}
                val label=label(focused)
                observed.add(label)
                if(label=="Deny")break
                advanceFocus(automation,focused)
                SystemClock.sleep(250)
            }
            assertTrue("TalkBack did not traverse command text: $observed",observed.any {it.contains("review the complete fixture command")})
            assertTrue("TalkBack did not traverse the allow action: $observed",observed.any {it.contains("Run this shell line once")})
            assertTrue("TalkBack did not traverse Deny: $observed",observed.any {it=="Deny"})
            scenario.onActivity {assertNull("Traversal must not submit a decision",it.fixtureDecision)}
            val deny=nodes(automation).first {it.isAccessibilityFocused && label(it)=="Deny"}
            assertTrue(deny.isEnabled)
            assertTrue("Explicit native decision click",deny.performAction(AccessibilityNodeInfo.ACTION_CLICK))
            instrumentation.waitForIdleSync()
            scenario.onActivity {assertEquals("fixture-deny",it.fixtureDecision)}
        } finally {
            try {
                try {scenario?.onActivity {it.finishAndRemoveTask()}}
                finally {scenario?.close()}
            } finally {
                settings(previous)
                waitFor("Restored accessibility settings") {keys.all {Settings.Secure.getString(resolver,it)==previous[it]}}
                assertEquals("TalkBack notification permission/flags preserved",notificationBefore,notificationState())
            }
        }
    }
    private fun checkPage(page:String,automation:UiAutomation,scenario:ActivityScenario<UiAccessibilityFixtureActivity>) {
        when(page) {
            "logs"->{
                val warning=traverseTo(automation) {it=="Warnings"}
                assertFalse(warning.isSelected)
                assertTrue(warning.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Warnings tab selected") {nodes(automation).any {label(it)=="Warnings" && it.isSelected}}
                assertNotNull(find(automation,"Fixture reconnect waiting"))
                assertNull(find(automation,"Fixture connection unavailable"))
                val live=traverseTo(automation,nodeFilter={it.isCheckable}) {it=="Live updates"}
                assertTrue("Live switch exposes checked state",live.isCheckable && live.isChecked)
                assertTrue(live.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Live updates paused") {nodes(automation).any {label(it)=="Live updates" && it.isCheckable && !it.isChecked}}
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            }
            "agents","agents-saved","agents-error"->{
                val status=if(page=="agents-error")"Configuration problem" else "Available"
                if(page=="agents-saved")traverseTo(automation) {it.startsWith("Saved agent list")}
                val row=traverseTo(automation) {it.contains("Fixture agent") && it.contains(status)}
                assertTrue("Agent exposes inspect action",row.actionList.any {it.label?.toString()=="Inspect agent"})
                assertTrue(row.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Agent details") {find(automation,"Agent details")!=null}
                assertTrue(find(automation,"Agent details")!!.isHeading)
                if(page!="agents") {
                    val launch=traverseTo(automation) {it=="New session"}
                    assertFalse("Cached/misconfigured discovery cannot launch",launch.isEnabled)
                    scenario.onActivity {assertFalse(it.fixtureActions.contains("new-session"))}
                }
                val back=nodes(automation).first {it.isClickable && label(it)=="Back"}
                assertTrue(back.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Agents returned") {find(automation,"Agents")!=null}
                waitFor("Automatic focus on returned Agents page") {
                    val current=label(nodes(automation).firstOrNull {it.isAccessibilityFocused})
                    current in setOf("Agents","Back","Refresh agents","Fixture computer") ||
                        current.contains("Fixture agent") && current.contains(status)
                }
                reportRecoveredFocus(automation)
                traverseTo(automation) {it.contains("Fixture agent") && it.contains(status)}
                scenario.onActivity {assertEquals(listOf("inspect:fixture-agent","agent-back"),it.fixtureActions)}
            }
            "recovery"->{
                val dismiss=traverseTo(automation) {it=="Dismiss"}
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
                assertTrue(dismiss.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Error dismissed") {find(automation,"Dismiss")==null}
                val heading=find(automation,"Connection lost")!!
                assertTrue(heading.isHeading)
                waitFor("Automatic focus survives error dismissal") {
                    label(nodes(automation).firstOrNull {it.isAccessibilityFocused}) in setOf(
                        "Recovery fixture","Connection lost","Reconnect",
                        "Reconnect to continue. Your draft and selected files stay on this device.")
                }
                reportRecoveredFocus(automation)
                val reconnect=traverseTo(automation) {it=="Reconnect"}
                scenario.onActivity {assertEquals(listOf("dismiss"),it.fixtureActions)}
                assertTrue(reconnect.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                scenario.onActivity {assertEquals(listOf("dismiss","reconnect"),it.fixtureActions)}
            }
            "tools"->{
                val expand=traverseTo(automation) {it=="Expand tool: Review allow command"}
                assertEquals("Collapsed",expand.stateDescription?.toString())
                assertTrue(expand.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                waitFor("Tool expanded with retained focus") {nodes(automation).any {it.isAccessibilityFocused && label(it)=="Collapse tool: Review allow command" && it.stateDescription?.toString()=="Expanded"}}
                traverseTo(automation) {it=="Reason: Fixture allow explanation"}
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            }
            "badges"->{
                for(decision in listOf("deny","ask","reviewing")) {
                    val expand=traverseTo(automation) {it=="Expand tool: Review $decision command"}
                    assertEquals("Collapsed",expand.stateDescription?.toString())
                    assertTrue(expand.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                    waitFor("$decision disclosure keeps focus") {nodes(automation).any {it.isAccessibilityFocused && label(it)=="Collapse tool: Review $decision command" && it.stateDescription?.toString()=="Expanded"}}
                    traverseTo(automation) {it=="Reason: Fixture $decision explanation"}
                }
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            }
            "notices"->{
                val warning=traverseTo(automation) {it.contains(dev.acportal.protocol.OWN_TOOLS_LABEL)}
                assertEquals("Collapsed",warning.stateDescription?.toString())
                assertTrue(warning.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                traverseTo(automation) {it==dev.acportal.protocol.OWN_TOOLS_WARNING}
                traverseTo(automation) {it.contains(dev.acportal.protocol.hostWriteLabel(dev.acportal.protocol.HOST_WRITE_CHANGED)!!)}
                traverseTo(automation) {it==dev.acportal.protocol.hostWriteExplanation(dev.acportal.protocol.HOST_WRITE_CHANGED)}
                val expand=traverseTo(automation) {it=="Expand tool: Fixture unchanged write"}
                assertTrue(expand.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                traverseTo(automation) {it==dev.acportal.protocol.hostWriteExplanation(dev.acportal.protocol.HOST_WRITE_UNCHANGED)}
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
            }
            "elicitation"->{
                traverseTo(automation) {it=="Fixture lifecycle question"}
                traverseTo(automation) {it.contains("Fixture name")}
                val cancel=traverseTo(automation) {it=="Cancel"}
                scenario.onActivity {assertTrue(it.fixtureActions.isEmpty())}
                assertTrue(cancel.performAction(AccessibilityNodeInfo.ACTION_CLICK))
                scenario.onActivity {assertEquals(listOf("elicitation:cancel"),it.fixtureActions)}
            }
        }
    }
    private fun reportRecoveredFocus(automation:UiAutomation) {
        InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {
            putString("acportalRecoveredFocus",label(nodes(automation).firstOrNull {it.isAccessibilityFocused}))
        })
    }
    private fun traverseTo(automation:UiAutomation,nodeFilter:(AccessibilityNodeInfo)->Boolean={true},predicate:(String)->Boolean):AccessibilityNodeInfo {
        val observed=linkedSetOf<String>()
        for(step in 0 until 32) {
            val focused=nodes(automation).firstOrNull {it.isAccessibilityFocused}
            val current=label(focused)
            observed.add(current)
            if(focused!=null && predicate(current) && nodeFilter(focused))return focused
            advanceFocus(automation,focused)
        }
        error("TalkBack did not reach target: $observed")
    }
    private fun advanceFocus(automation:UiAutomation,focused:AccessibilityNodeInfo?) {
        fun changed():Boolean {
            val next=nodes(automation).firstOrNull {it.isAccessibilityFocused}
            return next!=null && next!=focused
        }
        // The emulator occasionally drops a hardware swipe. Retry navigation only;
        // never retry a click, answer or other mutation, and still require real focus.
        repeat(3) {attempt->
            if(changed())return
            InstrumentationRegistry.getInstrumentation().sendStatus(2,android.os.Bundle().apply {
                putString("acportalFocus",label(focused));putString("acportalGesture","swipeRight")
                putString("acportalGestureAttempt",(attempt+1).toString())
            })
            val deadline=SystemClock.uptimeMillis()+5_000
            while(!changed() && SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100)
            if(changed())return
        }
        fail("Hardware gesture changes native focus from ${label(focused)}")
    }
    private fun find(automation:UiAutomation,text:String):AccessibilityNodeInfo? = nodes(automation)
        .firstOrNull {it.text?.toString()==text || it.contentDescription?.toString()==text}
    private fun label(root:AccessibilityNodeInfo?):String {
        if(root==null)return ""
        val pending=java.util.ArrayDeque<AccessibilityNodeInfo>().apply {add(root)}
        val labels=linkedSetOf<String>()
        var visited=0
        while(pending.isNotEmpty() && visited++<64) {
            val node=pending.removeFirst()
            listOfNotNull(node.text,node.contentDescription).forEach {labels.add(it.toString())}
            for(index in 0 until node.childCount)node.getChild(index)?.let {pending.add(it)}
        }
        return labels.joinToString(" ")
    }
    private fun nodes(automation:UiAutomation):List<AccessibilityNodeInfo> {
        val queue=java.util.ArrayDeque<AccessibilityNodeInfo>()
        automation.rootInActiveWindow?.let {queue.add(it)}
        val result=mutableListOf<AccessibilityNodeInfo>()
        while(queue.isNotEmpty() && result.size<512) {
            val node=queue.removeFirst();result.add(node)
            for(index in 0 until node.childCount)node.getChild(index)?.let {queue.add(it)}
        }
        return result
    }
    private fun waitFor(description:String,condition:()->Boolean) {
        val deadline=SystemClock.uptimeMillis()+15_000
        while(!condition() && SystemClock.uptimeMillis()<deadline)SystemClock.sleep(100)
        assertTrue(description,condition())
    }
}
