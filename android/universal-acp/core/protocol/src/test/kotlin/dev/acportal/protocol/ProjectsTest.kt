package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class ProjectsTest {
    private val windows = HostProjects(listOf("C:\\Users\\me\\acportal", "C:\\Users\\me\\acportal\\android"), "C:\\Users\\me\\acportal\\.local\\state\\scratch")
    @Test fun aWindowsSessionInItsVerbatimFormMatchesTheProjectListedPlainly() {
        assertEquals("C:\\Users\\me\\acportal", owningProject("\\\\?\\C:\\Users\\me\\acportal", windows))
        assertEquals("C:\\Users\\me\\acportal", owningProject("\\\\?\\C:\\Users\\me\\acportal\\desktop", windows))
        assertEquals("C:\\Users\\me\\acportal\\android", owningProject("\\\\?\\C:\\Users\\me\\acportal\\android\\app", windows))
        assertEquals("\\\\srv\\share\\x", plainPath("\\\\?\\UNC\\srv\\share\\x"))
    }
    @Test fun aScratchFolderIsNoProjectEvenInsideAProject() {
        assertNull(owningProject("\\\\?\\C:\\Users\\me\\acportal\\.local\\state\\scratch\\1760000000-a1b2c3", windows))
        assertTrue(isScratchFolder("/home/x/.local/state/acpd/scratch/1760000000-a1b2c3", null))
        assertFalse(isScratchFolder("/home/x/scratch/notes", null))
    }
    @Test fun aFolderThatIsNoLongerAllowedBelongsToNoProject() {
        assertNull(owningProject("D:\\elsewhere", windows))
        assertEquals("acportal", folderName("\\\\?\\C:\\Users\\me\\acportal\\"))
    }
    @Test fun onlyWholeFolderNamesMatch() {
        assertNull(owningProject("/work/app2", HostProjects(listOf("/work/app"))))
        assertEquals("/work/app", owningProject("/work/APP/src", HostProjects(listOf("/work/app"))))
    }
}
