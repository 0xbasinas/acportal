package dev.acportal.transport

data class ConnectionDiagnostic(val time:Long,val level:String,val message:String)

/** Only fixed lifecycle messages belong here; never include network exceptions or wire data. */
fun connectionDiagnostic(state:ConnectionState,time:Long=System.currentTimeMillis()):ConnectionDiagnostic =
    when(state) {
        ConnectionState.Disconnected -> ConnectionDiagnostic(time,"Info","Disconnected from host")
        ConnectionState.Connecting -> ConnectionDiagnostic(time,"Info","Connecting to host")
        ConnectionState.Connected -> ConnectionDiagnostic(time,"Info","WebSocket connected")
        is ConnectionState.Reconnecting -> ConnectionDiagnostic(time,"Warning","Reconnecting, attempt ${state.attempt}")
        is ConnectionState.Failed -> ConnectionDiagnostic(time,"Error","Connection unavailable. Review connection status.")
    }

fun appendDiagnostic(events:List<ConnectionDiagnostic>,event:ConnectionDiagnostic):List<ConnectionDiagnostic> =
    (events.takeLast(199)+event)
