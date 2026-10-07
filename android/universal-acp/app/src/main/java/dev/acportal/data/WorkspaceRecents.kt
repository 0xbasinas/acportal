package dev.acportal.data

import dev.acportal.protocol.Workspace
import dev.acportal.protocol.canStartSession
import dev.acportal.storage.RecentWorkspace

/** A picker filter, not the filesystem security boundary. The host validates each launch. */
fun workspaceInCurrentRoots(hostId:String,path:String,roots:List<Workspace>):Boolean {
    if(path.isBlank() || path.replace('\\','/').split('/').any {it=="." || it==".."})return false
    val candidate=workspacePolicyKey(hostId,path)
    return roots.any {root->
        val allowed=workspacePolicyKey(hostId,root.path)
        candidate==allowed || candidate.startsWith(allowed.trimEnd('/')+"/")
    }
}
fun recentWorkspaceChoices(details:HostDetails,agentId:String?=null):List<RecentWorkspace> = details.recent
    .filter {it.hostId==details.host.id && (agentId==null || it.agentId==agentId) && details.agents.any {agent->agent.id==it.agentId && agent.canStartSession()} && workspaceInCurrentRoots(details.host.id,it.path,details.workspaces)}
    .sortedByDescending {it.lastUsed}
    .distinctBy {workspacePolicyKey(it.hostId,it.path) to it.agentId}
