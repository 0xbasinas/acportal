package dev.acportal.presentation

import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.ui.Modifier
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.navigation3.runtime.entryProvider
import androidx.navigation3.ui.NavDisplay
import org.junit.Rule
import org.junit.Test

/** Isolated clock-driven timing check using the same transition as production. */
class PortalMotionUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun darkPushAndBackSettleWithin240Millis()=check("dark")
    @Test fun lightPushAndBackSettleWithin240Millis()=check("light")
    private fun check(theme:String) {
        val stack=mutableStateListOf("first")
        compose.setContent {PortalTheme(theme) {
            NavDisplay(backStack=stack,onBack={stack.removeAt(stack.lastIndex)},modifier=Modifier.fillMaxSize(),
                sizeTransform=null,
                transitionSpec={portalPageTransition()},popTransitionSpec={portalPageTransition()},
                entryProvider=entryProvider {
                    entry<String> {key->Button({if(key=="first")stack.add("second") else stack.removeAt(stack.lastIndex)}) {Text(key)}}
                })
        }}
        compose.mainClock.autoAdvance=false
        try {
        compose.onNodeWithText("first").performClick()
        frame(2)
        compose.onNodeWithText("second").assertExists()
        frame(12)
        compose.onNodeWithText("second").assertIsDisplayed()
        compose.onNodeWithText("first").assertDoesNotExist()
        compose.onNodeWithText("second").performClick()
        frame(14)
        compose.onNodeWithText("first").assertIsDisplayed()
        compose.onNodeWithText("second").assertDoesNotExist()
        } finally {compose.mainClock.autoAdvance=true}
    }
    private fun frame(count:Int) {repeat(count) {compose.mainClock.advanceTimeByFrame();compose.waitForIdle()}}
}
