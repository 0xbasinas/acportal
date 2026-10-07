package dev.acportal.protocol

import org.junit.Assert.*
import org.junit.Test

class DiffTest {
    @Test fun collapsingKeepsEveryChangeAndExpansionRestoresOriginalNumbers() {
        val old=(1..30).joinToString("\n") {"line $it"}
        val new=old.replace("line 8\n","changed 8\n").replace("line 24\n","changed 24\n")
        val lines=unifiedDiff(old,new)
        val rows=diffRows(lines)
        assertEquals(2,diffChangeStarts(lines).size)
        assertEquals(lines.filter {it.kind!=DiffKind.CONTEXT},rows.filterIsInstance<DiffRow.Line>().map {it.line}.filter {it.kind!=DiffKind.CONTEXT})
        val expanded=diffRows(lines,rows.filterIsInstance<DiffRow.Gap>().map {it.start}.toSet())
        assertEquals(lines,expanded.filterIsInstance<DiffRow.Line>().map {it.line})
        assertEquals(30,expanded.filterIsInstance<DiffRow.Line>().last().line.newLine)
    }
    @Test fun emptyFileCreationAndDeletionHaveNoInventedBlankLines() {
        assertTrue(unifiedDiff(null,"").isEmpty())
        assertTrue(unifiedDiff("","").isEmpty())
        assertEquals(listOf(DiffLine(DiffKind.REMOVED,"old",1,null)),unifiedDiff("old",""))
        assertEquals(listOf(DiffLine(DiffKind.ADDED,"new",null,1)),unifiedDiff("","new"))
    }
    @Test fun unchangedLargeFileCanExpandWithoutLosingContent() {
        val lines=unifiedDiff("one\ntwo\nthree","one\ntwo\nthree")
        assertTrue(diffChangeStarts(lines).isEmpty())
        assertEquals(listOf(DiffRow.Gap(0,3)),diffRows(lines))
        assertEquals(lines,diffRows(lines,setOf(0)).filterIsInstance<DiffRow.Line>().map {it.line})
    }
    @Test fun largeUnchangedFilesAndSmallMiddleEditsDoNotBecomeWholeFileReplacements() {
        val old=(1..2000).joinToString("\n") {"line $it"}
        val unchanged=unifiedDiff(old,old)
        assertEquals(2000,unchanged.size)
        assertTrue(diffChangeStarts(unchanged).isEmpty())
        val edited=unifiedDiff(old,old.replace("line 999\n","extra\nline 999\n"))
        assertEquals(listOf("extra"),edited.filter {it.kind==DiffKind.ADDED}.map {it.text})
        assertTrue(edited.none {it.kind==DiffKind.REMOVED})
        assertEquals(2000,edited.last().oldLine)
        assertEquals(2001,edited.last().newLine)
    }
}
