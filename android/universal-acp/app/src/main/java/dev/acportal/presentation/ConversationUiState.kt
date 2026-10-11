package dev.acportal.presentation

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.SaveableStateHolder
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.saveable.rememberSaveableStateHolder

/** Route-owned presentation choices survive a cached-to-live content transition. */
class ConversationUiState internal constructor(
    internal val list:LazyListState,
    internal val disclosures:MutableState<List<String>>,
    internal val followLatest:MutableState<Boolean>,
    internal val historyVisible:MutableState<Boolean>,
    internal val items:SaveableStateHolder,
)
@Composable internal fun rememberConversationUiState(identity:String):ConversationUiState {
    val list=rememberLazyListState()
    val disclosures=rememberSaveable(identity) {mutableStateOf(emptyList<String>())}
    val followLatest=rememberSaveable(identity) {mutableStateOf(true)}
    // Only the visibility flag is saved. Prompt text/selection remains in the
    // existing history owner and no submission is restored or sent.
    val historyVisible=rememberSaveable(identity) {mutableStateOf(false)}
    val items=rememberSaveableStateHolder()
    return remember(identity,list,disclosures,followLatest,historyVisible,items) {ConversationUiState(list,disclosures,followLatest,historyVisible,items)}
}
