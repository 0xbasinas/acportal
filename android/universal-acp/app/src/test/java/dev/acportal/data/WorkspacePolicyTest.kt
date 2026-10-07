package dev.acportal.data

import org.junit.Assert.*
import org.junit.Test

class WorkspacePolicyTest {
    @Test fun windowsPathsUseTheSamePolicyAcrossCanonicalizationAndCase() {
        assertEquals(workspacePolicyKey("host","C:/Projects/App/"),workspacePolicyKey("host","\\\\?\\C:\\projects\\app"))
        assertEquals(workspacePolicyKey("host","\\\\Server\\Share\\App"),workspacePolicyKey("host","\\\\?\\UNC\\server\\share\\app\\"))
    }
    @Test fun unixCaseSiblingFoldersAndHostsStayDistinct() {
        assertNotEquals(workspacePolicyKey("host","/projects/App"),workspacePolicyKey("host","/projects/app"))
        assertNotEquals(workspacePolicyKey("host","/projects/app"),workspacePolicyKey("host","/projects/app-two"))
        assertNotEquals(workspacePolicyKey("host","/projects/app"),workspacePolicyKey("other-host","/projects/app"))
        assertEquals("host|/",workspacePolicyKey("host","/"))
    }
}
