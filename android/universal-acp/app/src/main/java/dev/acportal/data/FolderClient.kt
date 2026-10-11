package dev.acportal.data

import dev.acportal.protocol.DirectoryListing
import dev.acportal.protocol.Workspace

/** What the add-folder dialog needs from the host, so it can be driven by a fake in tests. */
interface FolderClient {
    suspend fun directories(path:String?):DirectoryListing
    suspend fun add(path:String):Workspace
    suspend fun remove(path:String)
}
