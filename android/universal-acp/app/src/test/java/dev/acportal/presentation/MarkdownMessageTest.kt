package dev.acportal.presentation

import org.commonmark.node.*
import org.commonmark.ext.gfm.tables.TableBlock
import org.junit.Assert.*
import org.junit.Test

class MarkdownMessageTest {
    @Test fun commonmarkBlocksPreserveCodeAndNestedEmphasis() {
        val root=parseMessageMarkdown("# Heading\n\nA **bold and *italic*** reply.\n\n```kotlin\nval x = \"<html>\"\n```\n\n1. First\n2. Second\n\n> Quoted\n")!!
        val blocks=root.children()
        assertTrue(blocks[0] is Heading)
        assertEquals("A bold and italic reply.",markdownInline(blocks[1],{}).text)
        assertEquals("val x = \"<html>\"\n",(blocks[2] as FencedCodeBlock).literal)
        assertTrue(blocks[3] is OrderedList);assertTrue(blocks[4] is BlockQuote)
    }
    @Test fun tablesAreParsedAndImagesRemainText() {
        val root=parseMessageMarkdown("| Name | Value |\n| --- | --- |\n| alpha | **beta** |\n\n![remote](https://image.example/private.png)")!!
        assertTrue(root.children()[0] is TableBlock)
        assertEquals("Image: remote (https://image.example/private.png)",markdownInline(root.children()[1],{}).text)
    }
    @Test fun linksRequireWebSchemesAndNoEmbeddedCredentials() {
        assertEquals("https://example.com/path",markdownWebUrl("https://example.com/path"))
        listOf("javascript:alert(1)","file:///secret","intent://host","https://user:secret@example.com","https://example.com/"+"x".repeat(4096)).forEach {assertNull(markdownWebUrl(it))}
        val node=parseMessageMarkdown("[blocked](javascript:evil)")!!.firstChild
        assertEquals("blocked (javascript:evil)",markdownInline(node,{}).text)
    }
    @Test fun oversizedAndOverlyComplexFormattingFallsBackWithoutChangingSource() {
        assertNull(parseMessageMarkdown("x".repeat(65_537)))
        assertNull(parseMessageMarkdown((1..1001).joinToString("\n\n") {"paragraph $it"}))
        assertEquals("<script>alert(1)</script>",(parseMessageMarkdown("<script>alert(1)</script>")!!.firstChild as HtmlBlock).literal)
    }
}
