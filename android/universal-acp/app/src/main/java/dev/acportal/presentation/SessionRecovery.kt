package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import dev.acportal.transport.*

/** Recovery actions reconnect or navigate. Sending a turn remains a separate user action. */
@Composable fun SessionRecovery(
    state:SessionState,connection:ConnectionState,onDismiss:()->Unit,onReconnect:()->Unit,
    onSessions:()->Unit,onPairHost:(()->Unit)?=null,
) {
    val failure=connection as? ConnectionState.Failed
    val disconnected=connection is ConnectionState.Disconnected
    if(failure==null && !disconnected && state.error==null && !state.sessionClosed)return
    val title=when {
        failure?.kind==ConnectionFailure.AUTHORIZATION->"Pairing expired"
        failure?.kind==ConnectionFailure.SESSION_UNAVAILABLE->"Session unavailable"
        failure?.kind==ConnectionFailure.CONTROLLER_BUSY->"Session in use"
        state.sessionClosed->"Session stopped"
        failure!=null || disconnected->"Connection lost"
        state.errorOrigin==SessionErrorOrigin.AUTHENTICATION->"Sign-in failed"
        state.errorOrigin==SessionErrorOrigin.SESSION->"Session stopped"
        state.errorOrigin==SessionErrorOrigin.CONNECTION->"Restoring connection"
        state.errorOrigin==SessionErrorOrigin.PROTOCOL->"Host event failed"
        else->"Agent request failed"
    }
    val message=failure?.reason ?: if(state.sessionClosed)state.error ?: "Open Sessions to start or explicitly resume an agent." else if(disconnected)"Reconnect to continue. Your draft and selected files stay on this device." else state.error.orEmpty()
    Surface(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp),shape=RoundedCornerShape(12.dp),color=MaterialTheme.colorScheme.surfaceContainer) {
        Column(Modifier.heightIn(max=240.dp).verticalScroll(rememberScrollState()).padding(16.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            Text(title,style=MaterialTheme.typography.titleSmall)
            Text(message,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(state.error!=null && failure!=null)Text(state.error.orEmpty(),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            if(state.errorOrigin==SessionErrorOrigin.AUTHENTICATION && failure==null && !disconnected)Text("Choose a sign-in method below when you are ready to try again.",style=MaterialTheme.typography.bodySmall)
            Row(Modifier.fillMaxWidth(),horizontalArrangement=Arrangement.spacedBy(8.dp)) {
                when {
                    failure?.kind==ConnectionFailure.AUTHORIZATION->if(onPairHost!=null)TextButton(onPairHost) {Text("Pair again")} else TextButton(onSessions) {Text("Open Sessions")}
                    failure?.kind==ConnectionFailure.SESSION_UNAVAILABLE || state.sessionClosed || state.errorOrigin==SessionErrorOrigin.SESSION && state.error!=null->TextButton(onSessions) {Text("Open Sessions")}
                    failure!=null || disconnected->TextButton(onReconnect) {Text("Reconnect")}
                }
                if(state.error!=null)TextButton(onDismiss) {Text("Dismiss")}
            }
        }
    }
}

