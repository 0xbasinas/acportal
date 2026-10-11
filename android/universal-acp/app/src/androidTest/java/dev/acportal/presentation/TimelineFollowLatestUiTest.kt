package dev.acportal.presentation

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.runBlocking
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class TimelineFollowLatestUiTest {
    @get:Rule val compose=createComposeRule()

    private fun check(lastHeight:Int) {
        lateinit var state:androidx.compose.foundation.lazy.LazyListState
        compose.setContent {
            state=rememberLazyListState()
            Box(Modifier.fillMaxSize()) {LazyColumn(state=state,modifier=Modifier.fillMaxSize()) {
                items(3) {Box(Modifier.height(80.dp))}
                item {Box(Modifier.height(lastHeight.dp))}
            }}
        }
        compose.waitForIdle()
        runBlocking {compose.runOnUiThread {}}
        compose.runOnIdle {runBlocking {state.scrollToTimelineEnd(3)}}
        compose.waitForIdle()
        assertFalse("Follow-latest must reach the end of a tall last item",state.canScrollForward)
    }
    @Test fun tallLastItemEndsAtItsBottom()=check(4000)
    @Test fun shortLastItemStillEndsAtItsBottom()=check(40)
}
