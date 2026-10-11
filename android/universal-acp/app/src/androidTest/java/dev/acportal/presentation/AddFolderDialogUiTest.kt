package dev.acportal.presentation

import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import dev.acportal.data.FolderClient
import dev.acportal.protocol.DirectoryListing
import dev.acportal.protocol.Workspace
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class AddFolderDialogUiTest {
    @get:Rule val compose=createComposeRule()

    private class Fake:FolderClient {
        val added=mutableListOf<String>();val removed=mutableListOf<String>()
        override suspend fun directories(path:String?)=when(path) {
            null->DirectoryListing(null,null,listOf(Workspace("C:/","C:")))
            "C:/"->DirectoryListing("C:/",null,listOf(Workspace("C:/Users","Users")))
            else->DirectoryListing(path,"C:/",listOf(Workspace(path+"/work","work")))
        }
        override suspend fun add(path:String):Workspace {added+=path;return Workspace(path,path.substringAfterLast('@'),true)}
        override suspend fun remove(path:String) {removed+=path}
    }
    private fun check(theme:String) {
        val fake=Fake()
        var addedResult:Workspace?=null
        var removedCalls=0
        compose.setContent {PortalTheme(theme) {AddFolderDialog(fake,listOf(Workspace("D:/proj","proj",true)),{addedResult=it},{removedCalls++},{})}}
        compose.onNodeWithText("This computer").assertIsDisplayed()
        compose.onNodeWithTag("add-folder-confirm").assertIsNotEnabled()
        compose.onNodeWithText("C:").performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithText("Users").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("Users").performClick()
        compose.waitUntil(5000) {compose.onAllNodesWithText("work").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithTag("add-folder-confirm").assertIsEnabled().performClick()
        compose.waitUntil(5000) {addedResult!=null}
        assertEquals(listOf("C:/Users"),fake.added)
        // The folder the phone added earlier can be removed after confirmation.
        compose.onNodeWithText("Remove").performClick()
        compose.onNodeWithText("Remove folder?").assertIsDisplayed()
        compose.onNodeWithTag("remove-folder-confirm").performClick()
        compose.waitUntil(5000) {removedCalls==1}
        assertEquals(listOf("D:/proj"),fake.removed)
    }
    @Test fun darkBrowseAddAndRemove()=check("dark")
    @Test fun lightBrowseAddAndRemove()=check("light")
}
