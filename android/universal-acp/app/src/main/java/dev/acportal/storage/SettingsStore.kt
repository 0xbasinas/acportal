package dev.acportal.storage

import android.content.Context
import androidx.datastore.preferences.core.*
import androidx.datastore.preferences.preferencesDataStore
import androidx.datastore.core.DataStore
import kotlinx.coroutines.flow.map
import dev.acportal.protocol.WireJson
import kotlinx.serialization.encodeToString
import dev.acportal.protocol.WorkspaceAccess
import dev.acportal.protocol.TurnTimes
import dev.acportal.protocol.HostProjects
import dev.acportal.data.workspacePolicyKey

private val Context.portalSettings by preferencesDataStore("settings")
data class PortalSettings(val theme: String = "dark",val cacheMessages: Boolean = true)
class SettingsStore(context: Context,private val dataStore:DataStore<Preferences> = context.portalSettings) {
    private val theme = stringPreferencesKey("theme")
    private val cache = booleanPreferencesKey("cache_messages")
    private val mcp = stringPreferencesKey("mcp_aliases")
    private val access = stringPreferencesKey("workspace_access")
    val workspacePolicies = dataStore.data.map {WireJson.decodeFromString<Map<String,WorkspaceAccess>>(it[access] ?: "{}")}
    suspend fun workspaceAccess(hostId:String,path:String,value:WorkspaceAccess) {dataStore.edit {preferences->
        val policies=WireJson.decodeFromString<Map<String,WorkspaceAccess>>(preferences[access] ?: "{}").toMutableMap()
        policies[workspacePolicyKey(hostId,path)]=value
        require(policies.size<=512) {"Saved workspace access limit reached"}
        preferences[access]=WireJson.encodeToString(policies)
    }}
    suspend fun removeWorkspaceAccess(hostId:String) {dataStore.edit {preferences->
        val policies=WireJson.decodeFromString<Map<String,WorkspaceAccess>>(preferences[access] ?: "{}").filterKeys {!it.startsWith("$hostId|")}
        preferences[access]=WireJson.encodeToString(policies)
    }}
    private val projects = stringPreferencesKey("host_projects")
    /** The folders each host allows and where it keeps scratch folders, so Sessions can group by project. */
    val hostProjects = dataStore.data.map {preferences->try {WireJson.decodeFromString<Map<String,HostProjects>>(preferences[projects] ?: "{}")} catch(_:Exception) {emptyMap()}}
    suspend fun saveHostProjects(hostId:String,value:HostProjects) {dataStore.edit {preferences->
        val all=try {WireJson.decodeFromString<Map<String,HostProjects>>(preferences[projects] ?: "{}")} catch(_:Exception) {emptyMap()}
        if(all[hostId]!=value)preferences[projects]=WireJson.encodeToString(all+(hostId to value))
    }}
    private val turns = stringPreferencesKey("turn_times")
    /** How long each turn took on this phone, by session. Small and bounded; see [dev.acportal.protocol.TurnClock]. */
    val turnTimes = dataStore.data.map {preferences->try {WireJson.decodeFromString<TurnTimes>(preferences[turns] ?: "{}")} catch(_:Exception) {emptyMap()}}
    suspend fun saveTurnTimes(value:TurnTimes) {dataStore.edit {it[turns]=WireJson.encodeToString(value)}}
    val mcpAliases = dataStore.data.map { WireJson.decodeFromString<Map<String,String>>(it[mcp] ?: "{}") }
    suspend fun mcpAlias(hostId:String,alias:String?) { dataStore.edit { preferences->
        val aliases=WireJson.decodeFromString<Map<String,String>>(preferences[mcp] ?: "{}").toMutableMap()
        if(alias==null)aliases.remove(hostId) else aliases[hostId]=alias
        preferences[mcp]=WireJson.encodeToString(aliases)
    } }
    val settings = dataStore.data.map { PortalSettings(it[theme] ?: "dark",it[cache] ?: true) }
    suspend fun theme(value: String) { require(value in listOf("system","light","dark"));dataStore.edit { it[theme] = value } }
    suspend fun cache(value: Boolean) { dataStore.edit { it[cache] = value } }
}
