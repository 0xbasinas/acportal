package dev.acportal.data

import dev.acportal.protocol.SessionState
import dev.acportal.transport.ConnectionState

enum class LiveSessionActivity(val label:String) {
    READY("Ready"), WORKING("Working"), WAITING_APPROVAL("Waiting for approval"),
    SYNCING("Syncing"), CONNECTING("Connecting"), RECONNECTING("Reconnecting"),
    DISCONNECTED("Disconnected"), FAILED("Connection failed"), STOPPED("Stopped")
}

/** Connection and replay state must establish freshness before showing a turn as live. */
fun liveSessionActivity(state:SessionState,connection:ConnectionState):LiveSessionActivity = when {
    state.sessionClosed -> LiveSessionActivity.STOPPED
    connection is ConnectionState.Failed -> LiveSessionActivity.FAILED
    connection is ConnectionState.Reconnecting -> LiveSessionActivity.RECONNECTING
    connection == ConnectionState.Connecting -> LiveSessionActivity.CONNECTING
    connection != ConnectionState.Connected -> LiveSessionActivity.DISCONNECTED
    state.replaying -> LiveSessionActivity.SYNCING
    state.permissions.isNotEmpty() -> LiveSessionActivity.WAITING_APPROVAL
    state.processing -> LiveSessionActivity.WORKING
    else -> LiveSessionActivity.READY
}
