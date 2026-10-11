package dev.acportal.data

import dev.acportal.protocol.AgentCheck

/** What the user typed on the Add agent screen. The host validates it again. */
data class AgentRegistration(val id:String,val name:String,val command:String,val args:List<String> = emptyList(),val env:Map<String,String> = emptyMap(),val ownTools:Boolean=false)

/** What the Add agent screen needs from the host, so tests can drive it with a fake. */
interface AgentClient {
    suspend fun check(command:String,env:Map<String,String>):AgentCheck
    suspend fun add(registration:AgentRegistration)
    suspend fun remove(id:String)
}
