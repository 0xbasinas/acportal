package dev.acportal.protocol

enum class DiffKind { CONTEXT, ADDED, REMOVED }
data class DiffLine(val kind: DiffKind, val text: String, val oldLine: Int?, val newLine: Int?)

/** A bounded LCS diff. Large files use a linear fallback rather than quadratic memory. */
fun unifiedDiff(oldText: String?, newText: String): List<DiffLine> {
    val old = oldText?.takeIf {it.isNotEmpty()}?.lines().orEmpty()
    val new = newText.takeIf {it.isNotEmpty()}?.lines().orEmpty()
    val result = mutableListOf<DiffLine>()
    var prefix=0
    while(prefix<minOf(old.size,new.size) && old[prefix]==new[prefix]) {result+=DiffLine(DiffKind.CONTEXT,old[prefix],prefix+1,prefix+1);prefix++}
    var suffix=0
    while(suffix<minOf(old.size,new.size)-prefix && old[old.lastIndex-suffix]==new[new.lastIndex-suffix])suffix++
    val before=old.subList(prefix,old.size-suffix)
    val after=new.subList(prefix,new.size-suffix)
    if(before.size.toLong()*after.size>1_000_000) {
        result+=before.mapIndexed {i,line->DiffLine(DiffKind.REMOVED,line,prefix+i+1,null)}
        result+=after.mapIndexed {i,line->DiffLine(DiffKind.ADDED,line,null,prefix+i+1)}
    } else {
    val lengths = Array(before.size + 1) { IntArray(after.size + 1) }
    for (i in before.indices.reversed()) for (j in after.indices.reversed()) lengths[i][j] = if (before[i] == after[j]) 1 + lengths[i+1][j+1] else maxOf(lengths[i+1][j],lengths[i][j+1])
    var i = 0; var j = 0
    while (i < before.size || j < after.size) {
        if (i < before.size && j < after.size && before[i] == after[j]) { result += DiffLine(DiffKind.CONTEXT, before[i],prefix+i+1,prefix+j+1); i++; j++ }
        else if (j < after.size && (i == before.size || lengths[i][j+1] >= lengths[i+1][j])) { result += DiffLine(DiffKind.ADDED,after[j],null,prefix+j+1); j++ }
        else { result += DiffLine(DiffKind.REMOVED,before[i],prefix+i+1,null); i++ }
    }
    }
    for(index in 0 until suffix)result+=DiffLine(DiffKind.CONTEXT,old[old.size-suffix+index],old.size-suffix+index+1,new.size-suffix+index+1)
    return result
}

sealed interface DiffRow {
    data class Line(val index:Int,val line:DiffLine):DiffRow
    data class Gap(val start:Int,val end:Int):DiffRow {val count:Int get()=end-start}
}

/** Keep three context lines around edits. Expanded gaps retain their original line numbers. */
fun diffRows(lines:List<DiffLine>,expanded:Set<Int> = emptySet()):List<DiffRow> {
    val result=mutableListOf<DiffRow>()
    var index=0
    while(index<lines.size) {
        if(lines[index].kind!=DiffKind.CONTEXT) {result+=DiffRow.Line(index,lines[index]);index++;continue}
        val start=index
        while(index<lines.size && lines[index].kind==DiffKind.CONTEXT)index++
        val end=index
        val head=if(start>0)minOf(3,end-start) else 0
        val tail=if(end<lines.size)minOf(3,end-start-head) else 0
        val gapStart=start+head;val gapEnd=end-tail
        if(gapEnd-gapStart<=1 || gapStart in expanded) {
            for(row in start until end)result+=DiffRow.Line(row,lines[row])
        } else {
            for(row in start until gapStart)result+=DiffRow.Line(row,lines[row])
            result+=DiffRow.Gap(gapStart,gapEnd)
            for(row in gapEnd until end)result+=DiffRow.Line(row,lines[row])
        }
    }
    return result
}

fun diffChangeStarts(lines:List<DiffLine>):List<Int> = lines.indices.filter {lines[it].kind!=DiffKind.CONTEXT && (it==0 || lines[it-1].kind==DiffKind.CONTEXT)}

data class FileChange(val id:String,val path:String,val oldText:String?,val newText:String)
fun sessionFileChanges(state:SessionState):List<FileChange> = state.items.filterIsInstance<TimelineItem.Tool>().flatMap {fileChanges(it.id,it.content)}
fun permissionFileChanges(permission:Permission):List<FileChange> = fileChanges("permission-${permission.id}",permission.request["toolCall"].objectValue()["content"].arrayValue())
private fun fileChanges(id:String,content:kotlinx.serialization.json.JsonArray):List<FileChange> =
    content.mapIndexedNotNull {index,entry->
        val value=entry.objectValue()
        if(value["type"].text()!="diff")null
        else FileChange("$id/$index",value["path"].text(),value["oldText"]?.takeUnless {it==kotlinx.serialization.json.JsonNull}?.text(),value["newText"].text())
    }
