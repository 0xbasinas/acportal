package dev.acportal.presentation

import org.junit.Assert.assertEquals
import org.junit.Test

class FolderDisplayTest {
    @Test fun windowsPathsUseBackslashesAndUpperDrive() {
        assertEquals("C:\\Users\\basin\\ACPPortal-Test",displayPath("C:/Users/basin/ACPPortal-Test"))
        assertEquals("D:\\work",displayPath("d:/work"))
        assertEquals("C:\\",displayPath("C:/"))
    }
    @Test fun unixAndOtherPathsAreUnchanged() {
        assertEquals("/home/me/project",displayPath("/home/me/project"))
        assertEquals("relative/dir",displayPath("relative/dir"))
    }
    @Test fun titleFallsBackToLastSegment() {
        assertEquals("Project",folderTitle("Project","C:/x/y"))
        assertEquals("acportal",folderTitle("","C:/Users/basin/acportal/"))
        assertEquals("work",folderTitle("","D:\\work"))
        assertEquals("/",folderTitle("","/"))
    }
    @Test fun verbatimPrefixIsHidden() {
        assertEquals("C:\\Users\\me",displayPath("\\\\?\\C:\\Users\\me"))
        assertEquals("\\\\srv\\share",displayPath("\\\\?\\UNC\\srv\\share"))
    }
}
