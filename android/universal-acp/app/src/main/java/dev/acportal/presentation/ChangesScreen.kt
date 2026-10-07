package dev.acportal.presentation

import androidx.compose.foundation.background
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.outlined.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import dev.acportal.protocol.*
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

@Composable fun ChangesScreen(files:List<FileChange>,initialId:String,onBack:()->Unit) {
    var selected by rememberSaveable(initialId) {mutableStateOf(initialId)}
    var picker by remember {mutableStateOf(false)}
    val file=files.firstOrNull {it.id==selected} ?: files.firstOrNull()
    Surface(Modifier.fillMaxSize()) {
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Changes",onBack=onBack)
        if(file==null)EmptyState("No changes available","The change is no longer in this device's conversation cache.")
        else {
            Box(Modifier.padding(horizontal=16.dp)) {
                TextButton({picker=true},Modifier.fillMaxWidth().heightIn(min=48.dp)) {
                    Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(4.dp)) {
                        Text(changeFileName(file.path),style=MaterialTheme.typography.bodyMedium,maxLines=1,overflow=TextOverflow.Ellipsis)
                        if(file.path.isNotBlank() && file.path!=changeFileName(file.path))Text(file.path,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=1,overflow=TextOverflow.Ellipsis)
                    }
                    Icon(Icons.Outlined.ExpandMore,"Choose changed file")
                }
                DropdownMenu(picker,{picker=false}) {
                    files.forEachIndexed {index,change->DropdownMenuItem(text={Column(verticalArrangement=Arrangement.spacedBy(4.dp)) {
                        Text("${index+1}. ${changeFileName(change.path)}",maxLines=1,overflow=TextOverflow.Ellipsis)
                        if(change.path.isNotBlank())Text(change.path,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant,maxLines=2,overflow=TextOverflow.Ellipsis)
                    }},onClick={selected=change.id;picker=false})}
                }
            }
            key(file.id) {FullDiff(file)}
        }
    }
    }
}

private fun changeFileName(path:String)=path.substringAfterLast('/').substringAfterLast('\\').ifBlank {"Unnamed file"}

@Composable private fun ColumnScope.FullDiff(file:FileChange) {
    val lines by produceState<List<DiffLine>?>(null,file.oldText,file.newText) {value=null;value=withContext(Dispatchers.Default) {unifiedDiff(file.oldText,file.newText)}}
    val current=lines
    if(current==null) {Box(Modifier.weight(1f).fillMaxWidth(),contentAlignment=Alignment.Center) {CircularProgressIndicator()};return}
    var expanded by remember(file.id,current) {mutableStateOf(emptySet<Int>())}
    val rows=remember(current,expanded) {diffRows(current,expanded)}
    val gaps=remember(current) {diffRows(current).filterIsInstance<DiffRow.Gap>().map {it.start}.toSet()}
    val changes=remember(current) {diffChangeStarts(current)}
    val list=rememberLazyListState()
    val scope=rememberCoroutineScope()
    var target by rememberSaveable(file.id) {mutableIntStateOf(0)}
    LaunchedEffect(changes) {target=target.coerceIn(0,changes.lastIndex.coerceAtLeast(0))}
    var split by rememberSaveable(file.id) {mutableStateOf(false)}
    BoxWithConstraints(Modifier.weight(1f).fillMaxWidth()) {
        val wide=maxWidth>=700.dp
        val scrollControls=maxHeight<240.dp || LocalDensity.current.fontScale>1.3f
        val controls:@Composable ()->Unit={
            Column {
            Row(Modifier.fillMaxWidth().padding(horizontal=16.dp),verticalAlignment=Alignment.CenterVertically) {
                Text(if(changes.isEmpty())"No modifications" else "Change ${target+1} of ${changes.size}",Modifier.weight(1f),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                if(wide)TextButton({split=!split}) {Text(if(split)"Unified" else "Split view")}
                fun jump(next:Int) {target=next;val row=rows.indexOfFirst {it is DiffRow.Line && it.index==changes[next]};if(row>=0)scope.launch {list.animateScrollToItem(row+if(scrollControls)1 else 0)}}
                IconButton({jump(target-1)},enabled=changes.isNotEmpty() && target>0) {Icon(Icons.Outlined.KeyboardArrowUp,"Previous change")}
                IconButton({jump(target+1)},enabled=changes.isNotEmpty() && target<changes.lastIndex) {Icon(Icons.Outlined.KeyboardArrowDown,"Next change")}
            }
            if(gaps.isNotEmpty())TextButton({expanded=if(expanded.isEmpty())gaps else emptySet()},Modifier.padding(horizontal=16.dp).heightIn(min=48.dp)) {Text(if(expanded.isEmpty())"Expand unchanged lines" else "Collapse unchanged lines")}
            if(wide && split)Row(Modifier.fillMaxWidth().padding(horizontal=16.dp,vertical=8.dp)) {Text("Before",Modifier.weight(1f),style=MaterialTheme.typography.labelMedium);Text("After",Modifier.weight(1f),style=MaterialTheme.typography.labelMedium)}
            HorizontalDivider(color=MaterialTheme.colorScheme.outlineVariant)
            }
        }
        Column(Modifier.fillMaxSize()) {
            if(!scrollControls)controls()
            LazyColumn(state=list,modifier=Modifier.fillMaxSize(),contentPadding=PaddingValues(bottom=24.dp)) {
                if(scrollControls)item(key="diff-controls") {controls()}
                items(rows,key={when(it) {is DiffRow.Line->"line-${it.index}";is DiffRow.Gap->"gap-${it.start}"}}) {row->
                    when(row) {
                        is DiffRow.Gap->TextButton({expanded=expanded+row.start},Modifier.fillMaxWidth().heightIn(min=48.dp)) {Icon(Icons.Outlined.UnfoldMore,null,Modifier.size(18.dp));Spacer(Modifier.width(8.dp));Text("Show ${row.count} unchanged lines")}
                        is DiffRow.Line->if(wide && split)SplitDiffLine(row.line,file.path) else {
                            val line=row.line
                            Row(Modifier.fillMaxWidth().background(diffBackground(line.kind)).horizontalScroll(rememberScrollState()).padding(horizontal=16.dp,vertical=3.dp)) {
                                Text("${line.oldLine?.toString()?.padStart(4) ?: "    "} ${line.newLine?.toString()?.padStart(4) ?: "    "} ${diffPrefix(line.kind)} ",fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                                Text(highlightCode(line.text,file.path),fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                }
            }
        }
    }
}

@Composable private fun SplitDiffLine(line:DiffLine,path:String) {
    Row(Modifier.fillMaxWidth()) {
        listOf(line.oldLine to (line.kind!=DiffKind.ADDED),line.newLine to (line.kind!=DiffKind.REMOVED)).forEach { (number,show)->
            Row(Modifier.weight(1f).background(if(show)diffBackground(line.kind) else Color.Transparent).horizontalScroll(rememberScrollState()).padding(horizontal=16.dp,vertical=3.dp)) {
                Text("${number?.toString()?.padStart(4) ?: "    "} ",fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                Text(if(show)highlightCode(line.text,path) else AnnotatedString(""),fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodySmall)
            }
        }
    }
}

private fun diffPrefix(kind:DiffKind)=when(kind) {DiffKind.ADDED->"+";DiffKind.REMOVED->"−";else->" "}
@Composable private fun diffBackground(kind:DiffKind):Color=when(kind) {DiffKind.ADDED->ConnectedGreen.copy(alpha=0.12f);DiffKind.REMOVED->MaterialTheme.colorScheme.error.copy(alpha=0.12f);else->Color.Transparent}

/** Lightweight lexical colors; the source text remains unchanged. */
@Composable private fun highlightCode(text:String,path:String):AnnotatedString {
    val dark=MaterialTheme.colorScheme.surface.luminance()<0.5f
    return remember(text,path,dark) {
        val extension=path.substringAfterLast('.').lowercase()
        val hashComment=extension in setOf("py","rb","sh","yaml","yml","toml")
        if(extension !in setOf("kt","kts","java","js","jsx","ts","tsx","rs","py","go","swift","c","h","cpp","cs","rb","sh","json","yaml","yml","toml") || text.length>16_384)AnnotatedString(text)
        else {
            val expression=if(hashComment)HashSyntax else SlashSyntax
            val result=AnnotatedString.Builder(text)
            expression.findAll(text).forEach {match->
                val token=match.value
                val color=when {token.startsWith("//") || token.startsWith("#")->if(dark)Color(0xFF9C9CA4) else Color(0xFF696972)
                    token.startsWith('"') || token.startsWith('\'')->if(dark)Color(0xFF9BDAAF) else Color(0xFF28613C)
                    token.firstOrNull()?.isDigit()==true->if(dark)Color(0xFFDDC48C) else Color(0xFF765110)
                    else->if(dark)Color(0xFFC6AFF5) else Color(0xFF69439D)}
                result.addStyle(SpanStyle(color=color,fontWeight=if(token in Keywords)FontWeight.Medium else FontWeight.Normal),match.range.first,match.range.last+1)
            }
            result.toAnnotatedString()
        }
    }
}
private val Keywords=setOf("fun","val","var","class","interface","object","if","else","when","return","true","false","null","let","const","function","async","await","import","from","def","for","while","in","fn","pub","impl","struct","enum","match","use","package","public","private","static","void","new","try","catch","throw")
private val TokenPattern="\"(?:\\\\.|[^\"\\\\])*\"|'(?:\\\\.|[^'\\\\])*'|\\b(?:${Keywords.joinToString("|")})\\b|\\b\\d+(?:\\.\\d+)?\\b"
private val SlashSyntax=Regex("//.*|$TokenPattern")
private val HashSyntax=Regex("#.*|$TokenPattern")
