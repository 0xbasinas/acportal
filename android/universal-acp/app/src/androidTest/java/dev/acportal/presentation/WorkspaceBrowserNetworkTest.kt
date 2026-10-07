package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Density
import androidx.lifecycle.ViewModelStore
import androidx.room3.Room
import androidx.sqlite.driver.AndroidSQLiteDriver
import androidx.test.platform.app.InstrumentationRegistry
import dev.acportal.PortalApplication
import dev.acportal.data.PortalRepository
import dev.acportal.protocol.*
import dev.acportal.security.CredentialVault
import dev.acportal.storage.PortalDatabase
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.first
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Assume.assumeTrue
import org.junit.Rule
import org.junit.Test
import java.net.InetSocketAddress
import java.net.ServerSocket
import java.net.URI
import java.net.URLDecoder
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicBoolean

class WorkspaceBrowserNetworkTest {
    @get:Rule val compose=createComposeRule()
    @get:Rule val deadline=org.junit.rules.Timeout.seconds(60)

    @Test fun failedRetryAndReopenedDialogKeepTheLatestFoldersAfterADelayedResponse()=checkNetwork(1f)
    @Test fun largeTextKeepsNetworkRecoveryControlsReachable()=checkNetwork(2f)
    @Test fun workspaceAccessBrowserRecoversWithoutChangingSavedPolicy()=checkNetwork(1f,access=true)
    private fun checkNetwork(fontScale:Float,access:Boolean=false) {
        val context=InstrumentationRegistry.getInstrumentation().targetContext
        val actual=(context.applicationContext as PortalApplication).repository
        val paired=runBlocking {actual.hosts.first().firstOrNull()}
        assumeTrue("Requires a saved host credential",paired!=null)
        val originalPolicies=runBlocking {actual.settings.workspacePolicies.first()}
        val rootSlow="/workspace/slow";val rootFast="/workspace/fast"
        val slowReceived=CountDownLatch(1);val releaseSlow=CountDownLatch(1);val slowFinished=CountDownLatch(1)
        val fastRequests=AtomicInteger();val mutations=AtomicInteger()
        val workspaceFailure=AtomicBoolean(false)
        val server=ServerSocket().apply {bind(InetSocketAddress("127.0.0.1",0))}
        val workers=Executors.newFixedThreadPool(4)
        val listener=Thread {
            try {while(!server.isClosed) {
                val socket=server.accept()
                workers.submit {
                    socket.use {
                        try {
                            socket.soTimeout=3000
                            val reader=socket.getInputStream().bufferedReader()
                            val first=reader.readLine().split(' ')
                            while(!reader.readLine().isNullOrEmpty()) {}
                            val uri=URI(first[1])
                            var status=200
                            val body=if(first[0]!="GET") {mutations.incrementAndGet();status=409;"{}"} else when(uri.path) {
                                "/v1/status" -> WireJson.encodeToString(HostStatus(paired!!.id,"Browser fixture","test"))
                                "/v1/agents" -> WireJson.encodeToString(listOf(AgentInfo("mock","Mock ACP",installed=true,status="available")))
                                "/v1/workspaces" -> if(workspaceFailure.get()) {status=503;"{}"} else WireJson.encodeToString(listOf(Workspace(rootSlow,"Slow folder"),Workspace(rootFast,"Fast folder")))
                                "/v1/sessions" -> "[]"
                                "/v1/workspaces/browse" -> {
                                    val path=URLDecoder.decode(uri.rawQuery.substringAfter("path="),"UTF-8")
                                    if(path==rootSlow) {
                                        slowReceived.countDown();releaseSlow.await(20,TimeUnit.SECONDS)
                                        WireJson.encodeToString(listOf(Workspace("$rootSlow/old","Old slow result")))
                                    } else if(fastRequests.incrementAndGet()==1) {status=503;"{}"}
                                    else WireJson.encodeToString(listOf(Workspace("$rootFast/current","Current fast result")))
                                }
                                else -> {status=404;"{}"}
                            }
                            val bytes=body.toByteArray(Charsets.UTF_8)
                            socket.getOutputStream().write("HTTP/1.1 $status Fixture\r\nContent-Type: application/json\r\nContent-Length: ${bytes.size}\r\nConnection: close\r\n\r\n".toByteArray())
                            socket.getOutputStream().write(bytes);socket.getOutputStream().flush()
                        } catch(_:Exception) {} finally {if(releaseSlow.count==0L)slowFinished.countDown()}
                    }
                }
            }} catch(_:Exception) {}
        }.apply {isDaemon=true;start()}
        val database=Room.inMemoryDatabaseBuilder<PortalDatabase>(context).setDriver(AndroidSQLiteDriver()).build()
        val scope=CoroutineScope(SupervisorJob()+Dispatchers.IO)
        val store=ViewModelStore()
        val fixtureHost=paired!!.copy(label="Browser fixture",address="http://127.0.0.1:${server.localPort}")
        try {
            runBlocking {database.portal().saveHost(fixtureHost)}
            val repository=PortalRepository(database.portal(),actual.api,CredentialVault(context),actual.settings,scope)
            val vm=PortalViewModel(repository);store.put("browser",vm)
            compose.setContent {
                val density=LocalDensity.current
                CompositionLocalProvider(LocalDensity provides Density(density.density,fontScale)) {PortalTheme {PortalNavigation(vm)}}
            }
            compose.waitUntil(15_000) {compose.onAllNodesWithText("Browser fixture").fetchSemanticsNodes().isNotEmpty() && !vm.ui.value.loading}
            if(access) {
                compose.onNodeWithContentDescription("Settings tab").performClick()
                compose.onNodeWithText("Workspace access").performClick()
                workspaceFailure.set(true)
                compose.onNodeWithText("Browser fixture").performClick()
                compose.waitUntil(15_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
                compose.onNodeWithText("Workspaces unavailable").assertIsDisplayed()
                compose.onNodeWithText("Loading workspaces").assertDoesNotExist()
                compose.onNodeWithContentDescription("Dismiss error").performClick()
                compose.onNodeWithText("Reload workspaces").assertIsDisplayed()
                workspaceFailure.set(false)
                compose.waitForIdle()
                compose.onNodeWithText("Workspaces unavailable").assertIsDisplayed()
                compose.onNodeWithText("Reload workspaces").performClick()
                compose.waitUntil(15_000) {!vm.ui.value.loading && compose.onAllNodesWithText("Browse folders").fetchSemanticsNodes().isNotEmpty()}
            } else {
                compose.onNodeWithText("Browser fixture").performClick()
                compose.waitUntil(15_000) {!vm.ui.value.loading && compose.onAllNodesWithText("New session").fetchSemanticsNodes().isNotEmpty()}
                compose.onNodeWithText("New session").performClick()
                compose.onNodeWithTag("new-session-choices").performScrollToNode(hasText("Browse folders"))
            }
            compose.onNodeWithText("Browse folders").performClick()
            reveal(hasContentDescription("Browse Slow folder"))
            compose.onNodeWithContentDescription("Browse Slow folder").performClick()
            assertTrue(slowReceived.await(5,TimeUnit.SECONDS))
            compose.onNodeWithText("Loading folders").assertIsDisplayed()
            compose.onNodeWithText("Use this folder").assertIsNotEnabled()
            compose.onNodeWithText("Close").performClick()
            assertNull(vm.ui.value.workspaceBrowse.path)
            assertFalse(vm.ui.value.workspaceBrowse.loading)
            compose.onNodeWithText("Browse folders").performClick()
            reveal(hasContentDescription("Browse Fast folder"))
            compose.onNodeWithContentDescription("Browse Fast folder").performClick()
            compose.waitUntil(15_000) {compose.onAllNodesWithText("Could not open this folder").fetchSemanticsNodes().isNotEmpty()}
            assertNull(vm.ui.value.error)
            assertEquals(1,compose.onAllNodesWithText("Retry").fetchSemanticsNodes().size)
            compose.onNodeWithText("Use this folder").assertIsNotEnabled()
            if(!access)captureFailure(context,fontScale)
            reveal(hasText("Retry"))
            compose.onNode(hasText("Retry") and hasAnyAncestor(isDialog())).performClick()
            compose.waitUntil(15_000) {vm.ui.value.workspaceBrowse.folders?.singleOrNull()?.name=="Current fast result"}
            reveal(hasText("Current fast result"))
            releaseSlow.countDown()
            assertTrue(slowFinished.await(5,TimeUnit.SECONDS))
            // Let the old HTTP completion reach the ViewModel before asserting the current dialog.
            android.os.SystemClock.sleep(250)
            compose.onNodeWithText("Current fast result").assertIsDisplayed()
            assertEquals(rootFast,vm.ui.value.workspaceBrowse.path)
            compose.onNodeWithText("Old slow result").assertDoesNotExist()
            reveal(hasText("Use this folder"))
            compose.onNodeWithText("Use this folder").assertIsEnabled()
            compose.onNodeWithText("Close").performClick()
            compose.onNodeWithText("Browse folders").performClick()
            reveal(hasText("Slow folder"))
            compose.onNode(hasText("Slow folder") and hasAnyAncestor(isDialog())).assertIsDisplayed()
            reveal(hasText("Fast folder"))
            compose.onNode(hasText("Fast folder") and hasAnyAncestor(isDialog())).assertIsDisplayed()
            compose.onNodeWithText("Current fast result").assertDoesNotExist()
            if(access) {
                runBlocking {database.portal().deleteHost(fixtureHost.id)}
                compose.onNode(hasText("Fast folder") and hasAnyAncestor(isDialog())).performClick()
                compose.waitUntil(10_000) {!vm.ui.value.loading && vm.ui.value.error!=null}
                compose.onNodeWithText("Access unavailable").assertIsDisplayed()
                compose.onNodeWithText("Loading access").assertDoesNotExist()
                compose.onNodeWithContentDescription("Dismiss error").performClick()
                compose.onNodeWithText("Reload access").assertIsDisplayed()
                runBlocking {database.portal().saveHost(fixtureHost)}
                compose.waitForIdle()
                compose.onNodeWithText("Access unavailable").assertIsDisplayed()
                compose.onNodeWithText("Reload access").performClick()
                compose.waitUntil(10_000) {!vm.ui.value.loading && compose.onAllNodesWithText("Save access").fetchSemanticsNodes().isNotEmpty()}
                compose.onNodeWithContentDescription("Read files").assertIsEnabled()
                compose.onNodeWithContentDescription("Write files").assertIsEnabled()
                compose.onNodeWithContentDescription("Terminal").assertIsEnabled()
                assertEquals(rootFast,vm.ui.value.accessPath)
                assertEquals(originalPolicies,runBlocking {actual.settings.workspacePolicies.first()})
            }
            assertEquals(2,fastRequests.get());assertEquals(0,mutations.get())
        } finally {
            releaseSlow.countDown();store.clear();scope.cancel();server.close();workers.shutdownNow();listener.join(1000);database.close()
        }
    }
    private fun reveal(matcher:SemanticsMatcher) {
        compose.onNodeWithTag("workspace-browser-folders").performScrollToNode(matcher)
    }
    private fun captureFailure(context:android.content.Context,fontScale:Float) {
        compose.waitForIdle()
        val instrumentation=InstrumentationRegistry.getInstrumentation()
        val image=instrumentation.uiAutomation.takeScreenshot()
        val name=if(fontScale>1f)"workspace-browser-error-large-text" else "workspace-browser-error"
        val file=java.io.File(context.getExternalFilesDir("screenshots"),"$name.png")
        file.parentFile!!.mkdirs()
        file.outputStream().use {image.compress(android.graphics.Bitmap.CompressFormat.PNG,100,it)}
        image.recycle()
        instrumentation.uiAutomation.executeShellCommand("cp ${file.absolutePath} /sdcard/Download/acportal-$name.png").use {android.os.ParcelFileDescriptor.AutoCloseInputStream(it).readBytes()}
    }
}
