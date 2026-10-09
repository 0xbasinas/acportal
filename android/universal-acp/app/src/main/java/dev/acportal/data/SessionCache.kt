package dev.acportal.data

import dev.acportal.protocol.*
import kotlinx.serialization.json.*

/** Leave headroom in Android's cursor window for the other session columns. */
internal const val MAX_SESSION_CACHE_BYTES=1024*1024
internal fun sessionCache(state:SessionState,cache:Boolean):String {
    val info=buildJsonObject {
        state.sessionInfo["title"]?.let {put("title",it.text().take(256))}
        state.sessionInfo["updatedAt"]?.takeIf {it is JsonPrimitive && it.toString().length<=128}?.let {put("updatedAt",it)}
    }
    val checkpoint=SessionState(sequence=state.sequence,historyGap=state.historyGap,localHistoryCleared=state.localHistoryCleared || !cache,sessionInfo=info)
    // Decisions and model-request correlations are live-only. A restored controller
    // must obtain pending requests from the host, regardless of cache settings.
    val candidate=if(cache)state.copy(permissions=emptyMap(),replayPermissions=null,modelRequests=emptyMap()) else checkpoint
    return try {
        boundedJsonBytes(WireJson.encodeToJsonElement(candidate),MAX_SESSION_CACHE_BYTES,"Session cache exceeded the retained limit").toString(Charsets.UTF_8)
    } catch(_:IllegalArgumentException) {WireJson.encodeToString(checkpoint.copy(historyGap=true))}
}
