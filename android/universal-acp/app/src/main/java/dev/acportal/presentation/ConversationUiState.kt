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
    internal val items:SaveableStateHolder,
)
@Composable internal fun rememberConversationUiState(identity:String):ConversationUiState {
    val list=rememberLazyListState()
    val disclosures=rememberSaveable(identity) {mutableStateOf(emptyList<String>())}
    val followLatest=rememberSaveable(identity) {mutableStateOf(true)}
    val items=rememberSaveableStateHolder()
    return remember(identity,list,disclosures,followLatest,items) {ConversationUiState(list,disclosures,followLatest,items)}
}
