package dev.acportal.storage

import android.content.Context
import androidx.room3.*
import androidx.sqlite.driver.AndroidSQLiteDriver
import kotlinx.coroutines.flow.Flow

@Entity(tableName = "hosts")
data class HostProfile(
    @PrimaryKey val id: String,
    val label: String,
    val address: String,
    val credentialAlias: String,
    val deviceId: String,
    val expiresAt: Long,
    val lastSeen: Long = 0,
    val agentCount: Int = 0,
    val online: Boolean = false,
)
@Entity(tableName = "sessions", primaryKeys = ["hostId","id"], foreignKeys = [ForeignKey(entity=HostProfile::class,parentColumns=["id"],childColumns=["hostId"],onDelete=ForeignKey.CASCADE)], indices=[Index("hostId")])
data class StoredSession(val hostId: String, val id: String, val metadata: String, val state: String = "", val draft: String = "", val updatedAt: Long = System.currentTimeMillis(), val archived: Boolean = false)
@Entity(tableName = "recent_workspaces", primaryKeys = ["hostId","path","agentId"])
data class RecentWorkspace(val hostId: String, val path: String, val agentId: String, val lastUsed: Long = System.currentTimeMillis())

@Entity(tableName="agent_catalogs",foreignKeys=[ForeignKey(entity=HostProfile::class,parentColumns=["id"],childColumns=["hostId"],onDelete=ForeignKey.CASCADE)])
data class StoredAgentCatalog(@PrimaryKey val hostId:String,val metadata:String,val discoveredAt:Long)

/** One bounded row for page preferences, excluding actions, credentials and session payloads. */
@Entity(tableName="ui_state")
data class StoredUiState(@PrimaryKey val id:Int=1,val mainPage:String="connections",val sessionsArchived:Boolean=false,val sessionsActive:Boolean=false,val sessionsQuery:String="")

@Dao
interface PortalDao {
    @Query("SELECT * FROM ui_state WHERE id=1") fun uiState():Flow<StoredUiState?>
    @Insert(onConflict=OnConflictStrategy.IGNORE) suspend fun initializeUiState(state:StoredUiState)
    @Query("UPDATE ui_state SET mainPage=:page WHERE id=1") suspend fun updateMainPage(page:String)
    @Query("UPDATE ui_state SET sessionsArchived=:archived, sessionsActive=:active, sessionsQuery=:query WHERE id=1") suspend fun updateSessionFilters(archived:Boolean,active:Boolean,query:String)
    @Transaction suspend fun saveMainPage(page:String) {
        require(page in listOf("connections","sessions","settings"))
        initializeUiState(StoredUiState());updateMainPage(page)
    }
    @Transaction suspend fun saveSessionFilters(archived:Boolean,active:Boolean,query:String) {
        require(query.length<=512)
        initializeUiState(StoredUiState());updateSessionFilters(archived,active,query)
    }
    @Query("SELECT * FROM agent_catalogs") fun agentCatalogs():Flow<List<StoredAgentCatalog>>
    @Upsert suspend fun saveAgentCatalog(catalog:StoredAgentCatalog)
    @Query("SELECT * FROM hosts ORDER BY label COLLATE NOCASE") fun hosts(): Flow<List<HostProfile>>
    @Query("SELECT * FROM hosts WHERE id = :id") suspend fun host(id: String): HostProfile?
    @Upsert suspend fun saveHost(host: HostProfile)
    @Query("DELETE FROM hosts WHERE id = :id") suspend fun deleteHost(id: String)
    @Query("SELECT * FROM sessions ORDER BY updatedAt DESC") fun sessions(): Flow<List<StoredSession>>
    @Query("SELECT * FROM sessions WHERE hostId = :hostId AND id = :id") suspend fun session(hostId: String,id: String): StoredSession?
    @Upsert suspend fun saveSession(session: StoredSession)
    @Query("UPDATE sessions SET state = :state, updatedAt = :timestamp WHERE hostId = :hostId AND id = :id") suspend fun saveState(hostId:String,id:String,state:String,timestamp:Long)
    @Query("UPDATE sessions SET draft = :draft WHERE hostId = :hostId AND id = :id") suspend fun saveDraft(hostId:String,id:String,draft:String)
    @Transaction suspend fun clearLocalCopy(hostId:String,id:String,state:String,timestamp:Long) {
        saveState(hostId,id,state,timestamp)
        saveDraft(hostId,id,"")
    }
    @Query("UPDATE sessions SET archived = :archived WHERE hostId = :hostId AND id = :id") suspend fun archive(hostId:String,id:String,archived:Boolean)
    @Query("DELETE FROM sessions WHERE hostId = :hostId AND id = :id") suspend fun deleteSession(hostId:String,id:String)
    @Query("UPDATE sessions SET state = ''") suspend fun clearCache()
    @Upsert suspend fun saveRecent(workspace: RecentWorkspace)
    @Query("SELECT * FROM recent_workspaces WHERE hostId = :hostId ORDER BY lastUsed DESC LIMIT 20") suspend fun recent(hostId:String): List<RecentWorkspace>
    @Query("DELETE FROM recent_workspaces WHERE hostId = :hostId") suspend fun deleteRecent(hostId:String)
}
@Database(entities=[HostProfile::class,StoredSession::class,RecentWorkspace::class,StoredAgentCatalog::class,StoredUiState::class],version=3,exportSchema=true,autoMigrations=[AutoMigration(from=1,to=2),AutoMigration(from=2,to=3)])
abstract class PortalDatabase : RoomDatabase() {
    abstract fun portal(): PortalDao
    companion object {
        fun open(context: Context): PortalDatabase = Room.databaseBuilder<PortalDatabase>(context.applicationContext,"portal.db").setDriver(AndroidSQLiteDriver()).build()
    }
}
