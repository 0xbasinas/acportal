package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.runtime.*
import org.junit.Rule
import org.junit.Test

class MarkdownMessageUiTest {
    @get:Rule val compose=createComposeRule()
    @Test fun darkReplyRendersFormattingAndCode()=check("dark")
    @Test fun lightReplyRendersFormattingAndCode()=check("light")
    @Test fun continuousStreamingUpdatesTheParsedReplyBeforeTheStreamStops() {
        var source by mutableStateOf("# Stream\n\nInitial")
        compose.setContent {PortalTheme {MarkdownMessage(source)}}
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Stream").fetchSemanticsNodes().isNotEmpty()}
        compose.mainClock.autoAdvance=false
        try {
            repeat(30) {index->
                compose.runOnIdle {source+=" chunk$index"}
                compose.mainClock.advanceTimeBy(32)
                // Allow the off-Main parser to finish, without an 80 ms gap between chunks.
                Thread.sleep(20)
            }
            compose.onNodeWithText("chunk10",substring=true).assertExists()
        } finally {compose.mainClock.autoAdvance=true}
    }
    private fun check(theme:String) {
        val source="# Heading\n\nA **bold** reply with *emphasis*.\n\n```kotlin\nval x = 1\n```\n\n1. First\n2. Second\n\n> Quoted\n\n| Name | Value |\n| --- | --- |\n| alpha | beta |"
        compose.setContent {PortalTheme(theme) {Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState())) {MarkdownMessage(source)}}}
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Heading").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("A bold reply with emphasis.").assertIsDisplayed()
        compose.onNodeWithText("val x = 1\n").assertIsDisplayed()
        compose.onNodeWithText("First").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Quoted").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("alpha").performScrollTo().assertIsDisplayed()
    }
    @Test fun linksRequireConfirmationAndHtmlAndImagesStayText() {
        compose.setContent {PortalTheme {MarkdownMessage("[Website](https://example.com)\n\n<script>alert(1)</script>\n\n![remote](https://example.com/pixel.png)")}}
        compose.waitUntil(10_000) {compose.onAllNodesWithText("Website").fetchSemanticsNodes().isNotEmpty()}
        compose.onNodeWithText("Website").performClick()
        compose.onNodeWithText("Open link?").assertIsDisplayed()
        compose.onNodeWithText("Cancel").performClick()
        compose.onNodeWithText("<script>alert(1)</script>").assertIsDisplayed()
        compose.onNodeWithText("Image: remote (https://example.com/pixel.png)").assertIsDisplayed()
    }
}
