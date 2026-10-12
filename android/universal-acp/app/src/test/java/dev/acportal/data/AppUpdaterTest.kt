package dev.acportal.data

import kotlinx.coroutines.runBlocking
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.Buffer
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import java.io.IOException
import java.security.MessageDigest

class AppUpdaterTest {
    @get:Rule val temp=TemporaryFolder()

    private fun releaseJson(name:String="Debug build 0.1.0-debug.15+df33193",asset:String="acportal-debug-15-df33193.apk",url:String="https://github.com/x/releases/download/debug/acportal-debug-15-df33193.apk",size:Long=1000,digest:String?=null)=
        """{"name":"$name","published_at":"2026-10-11T15:02:00Z","assets":[{"name":"$asset","size":$size,"browser_download_url":"$url"${digest?.let {""","digest":"$it""""} ?: ""}}]}"""

    @Test fun releaseIsReadWithBuildNumberFromTheAssetName() {
        val sha="a".repeat(64)
        val release=parseRelease(releaseJson(digest="sha256:$sha"))
        assertEquals("0.1.0-debug.15+df33193",release.versionName)
        assertEquals(15L,release.build)
        assertEquals("acportal-debug-15-df33193.apk",release.assetName)
        assertEquals(sha,release.sha256)
        assertEquals(1000L,release.sizeBytes)
    }
    @Test fun buildNumberFallsBackToTheVersionName() {
        assertEquals(15L,parseRelease(releaseJson(asset="app.apk")).build)
    }
    @Test fun aMalformedChecksumIsIgnoredRatherThanTrusted() {
        assertNull(parseRelease(releaseJson(digest="sha256:nothex")).sha256)
        assertNull(parseRelease(releaseJson()).sha256)
    }
    @Test fun releasesWithoutAnApkOrABuildNumberAreRejected() {
        assertThrows(IllegalArgumentException::class.java) {parseRelease("""{"name":"Debug build x","assets":[]}""")}
        assertThrows(IllegalArgumentException::class.java) {parseRelease("not json")}
        assertThrows(IllegalArgumentException::class.java) {parseRelease(releaseJson(name="nothing",asset="app.apk"))}
    }
    @Test fun onlyAHigherBuildIsAnUpdate() {
        val release=parseRelease(releaseJson())
        assertTrue(isUpdate(14,release));assertFalse(isUpdate(15,release));assertFalse(isUpdate(16,release))
    }
    @Test fun onlyHttpsGithubAddressesAreTrusted() {
        fun trusted(url:String)=isTrustedUpdateAddress(url.toHttpUrl())
        assertTrue(trusted("https://github.com/a/b/releases/download/debug/x.apk"))
        assertTrue(trusted("https://release-assets.githubusercontent.com/x"))
        assertFalse(trusted("http://github.com/x.apk"))
        assertFalse(trusted("https://evil.example.com/x.apk"))
        assertFalse(trusted("https://github.com.evil.example/x.apk"))
        assertFalse(trusted("https://notgithubusercontent.com/x.apk"))
    }

    private fun signed(tag:String,name:String="acportal-x.apk",draft:Boolean=false,digest:String?=null)=
        """{"tag_name":"$tag","draft":$draft,"published_at":"2026-10-12T11:42:00Z","assets":[{"name":"$name","size":15000000,"browser_download_url":"https://github.com/x/releases/download/$tag/$name"${digest?.let {""","digest":"$it""""} ?: ""}},{"name":"$name.sha256","size":85,"browser_download_url":"https://github.com/x/$name.sha256"}]}"""

    @Test fun theHighestSignedReleaseIsChosenFromTheList() {
        val sha="b".repeat(64)
        val json="["+listOf(signed("v0.1.4"),signed("app-v0.1.3","acportal-0.1.3.apk"),signed("app-v0.1.10","acportal-0.1.10.apk",digest="sha256:$sha"),signed("app-v0.2.0","acportal-0.2.0.apk",draft=true),signed("debug","acportal-debug-9-abc.apk")).joinToString(",")+"]"
        val release=parseSignedRelease(json)
        assertEquals("0.1.10",release.versionName)
        assertEquals(110L,release.build)
        assertEquals("acportal-0.1.10.apk",release.assetName)
        assertEquals(sha,release.sha256)
    }
    @Test fun noSignedReleaseIsExplained() {
        val failure=assertThrows(IllegalArgumentException::class.java) {parseSignedRelease("["+signed("v0.1.4")+","+signed("debug")+"]")}
        assertTrue(failure.message!!.contains("No signed release"))
        assertThrows(IllegalArgumentException::class.java) {parseSignedRelease("not json")}
        assertThrows(IllegalArgumentException::class.java) {parseSignedRelease("{}")}
    }
    @Test fun versionCodeMatchesWhatTheReleaseWorkflowBuilds() {
        assertEquals(104L,parseSignedRelease("["+signed("app-v0.1.4","acportal-0.1.4.apk")+"]").build)
        assertEquals(10203L,parseSignedRelease("["+signed("app-v1.2.3","acportal-1.2.3.apk")+"]").build)
    }
    @Test fun theInstalledChannelComesFromTheVersionName() {
        assertEquals(UpdateChannel.Debug,UpdateChannel.installed("0.1.0-debug.15+df33193"))
        assertEquals(UpdateChannel.Release,UpdateChannel.installed("0.1.4"))
        assertEquals(UpdateChannel.Release,UpdateChannel.fromName("Release"))
        assertNull(UpdateChannel.fromName("beta"))
    }
    @Test fun otherChannelBuildsAreAlwaysOfferedAndSameChannelOnesCompareNumbers() {
        val debugInstalled=InstalledBuild("0.1.0-debug.15+abc",15)
        val releaseInstalled=InstalledBuild("0.1.4",104)
        val signedBuild=parseSignedRelease("["+signed("app-v0.1.4","acportal-0.1.4.apk")+"]")
        val debugBuild=parseRelease(releaseJson())
        assertTrue("a debug phone is offered the signed release",isUpdateFor(debugInstalled,UpdateChannel.Release,signedBuild))
        assertTrue("a release phone is offered a debug build even though 15 < 104",isUpdateFor(releaseInstalled,UpdateChannel.Debug,debugBuild))
        assertFalse(isUpdateFor(releaseInstalled,UpdateChannel.Release,signedBuild))
        assertFalse(isUpdateFor(debugInstalled,UpdateChannel.Debug,debugBuild))
        assertTrue(isUpdateFor(InstalledBuild("0.1.0-debug.12+abc",12),UpdateChannel.Debug,debugBuild))
    }

    private fun serve(body:ByteArray,block:(MockWebServer)->Unit) {
        MockWebServer().use {server->server.enqueue(MockResponse().setBody(Buffer().write(body)));server.start();block(server)}
    }
    private fun release(server:MockWebServer,body:ByteArray,sha:String?,size:Long=body.size.toLong())=
        ReleaseInfo("0.1.0-debug.15","15".toLong(),"x.apk",server.url("/x.apk").toString(),size,sha,null)
    private fun sha(bytes:ByteArray)=MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") {"%02x".format(it)}
    private fun updater(cache:java.io.File,accept:Boolean=true)=AppUpdater(OkHttpClient(),cache) {accept}

    @Test fun aMatchingDownloadIsKeptAndReportsProgress() {
        val bytes=ByteArray(200_000) {(it%251).toByte()}
        serve(bytes) {server ->
            var last=0L
            val file=runBlocking {updater(temp.root).download(release(server,bytes,sha(bytes))) {last=it}}
            assertArrayEquals(bytes,file.readBytes());assertEquals(bytes.size.toLong(),last)
            assertEquals("no partial file is left",listOf("update.apk"),file.parentFile.list()!!.toList())
        }
    }
    @Test fun aWrongChecksumIsDiscarded() {
        val bytes=ByteArray(5000) {1}
        serve(bytes) {server ->
            val failure=assertThrows(IOException::class.java) {runBlocking {updater(temp.root).download(release(server,bytes,"0".repeat(64))) {}}}
            assertTrue(failure.message!!.contains("checksum"))
            assertTrue(java.io.File(temp.root,"updates").list().isNullOrEmpty())
        }
    }
    @Test fun aShortDownloadIsDiscarded() {
        val bytes=ByteArray(5000) {1}
        serve(bytes) {server ->
            assertThrows(IOException::class.java) {runBlocking {updater(temp.root).download(release(server,bytes,null,size=9999)) {}}}
            assertTrue(java.io.File(temp.root,"updates").list().isNullOrEmpty())
        }
    }
    @Test fun anUntrustedAddressIsRefusedBeforeAnythingIsSaved() {
        val bytes=ByteArray(100)
        serve(bytes) {server ->
            val failure=assertThrows(IOException::class.java) {runBlocking {updater(temp.root,accept=false).download(release(server,bytes,null)) {}}}
            assertTrue(failure.message!!.contains("unexpected address"))
            assertTrue(java.io.File(temp.root,"updates").list().isNullOrEmpty())
        }
    }
    @Test fun anEarlierDownloadIsReplaced() {
        val old=java.io.File(temp.root,"updates").apply {mkdirs()}.resolve("update.apk").apply {writeText("old")}
        val bytes=ByteArray(300) {2}
        serve(bytes) {server ->
            val file=runBlocking {updater(temp.root).download(release(server,bytes,null)) {}}
            assertEquals(old.absolutePath,file.absolutePath);assertEquals(300L,file.length())
        }
    }
}
