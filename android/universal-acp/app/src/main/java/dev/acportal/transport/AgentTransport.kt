package dev.acportal.transport

import kotlinx.coroutines.flow.Flow

sealed interface ConnectionState {
    data object Disconnected : ConnectionState
    data object Connecting : ConnectionState
    data object Connected : ConnectionState
    data class Reconnecting(val attempt:Int) : ConnectionState
    data class Failed(val reason:String,val kind:ConnectionFailure=ConnectionFailure.CONNECTION) : ConnectionState
}
enum class ConnectionFailure { CONNECTION, AUTHORIZATION, SESSION_UNAVAILABLE, CONTROLLER_BUSY }
fun terminalConnectionFailure(code:Int?):ConnectionState.Failed? = when(code) {
    401,403->ConnectionState.Failed("Credentials expired. Pair the host again.",ConnectionFailure.AUTHORIZATION)
    404->ConnectionState.Failed("Session is no longer available. Open Sessions to continue.",ConnectionFailure.SESSION_UNAVAILABLE)
    409->ConnectionState.Failed("Another client controls this session. Disconnect it before reconnecting.",ConnectionFailure.CONTROLLER_BUSY)
    else->null
}
interface AgentTransport {
    suspend fun connect()
    suspend fun disconnect()
    suspend fun send(message:ByteArray)
    fun messages():Flow<ByteArray>
    fun connectionState():Flow<ConnectionState>
}
