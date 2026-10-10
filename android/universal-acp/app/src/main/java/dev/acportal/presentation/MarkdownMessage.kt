package dev.acportal.presentation

import android.content.Intent
import androidx.core.net.toUri
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.CustomAccessibilityAction
import androidx.compose.ui.semantics.customActions
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.*
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import org.commonmark.node.*
import org.commonmark.node.Text as MarkdownText
import org.commonmark.node.Paragraph as MarkdownParagraph
import org.commonmark.parser.Parser
import org.commonmark.ext.gfm.tables.*
import java.net.URI

private val markdownParser=Parser.builder().extensions(listOf(TablesExtension.create())).build()
internal fun Node.children():List<Node> = buildList {var child=firstChild;while(child!=null) {add(child);child=child.next}}

/** Presentation-only budget. Original source remains selectable and is never rewritten. */
internal fun parseMessageMarkdown(source:String):Node? {
    if(source.length>65_536)return null
    val root=markdownParser.parse(source)
    val pending=ArrayDeque<Pair<Node,Int>>();pending.add(root to 0)
    var count=0
    while(pending.isNotEmpty()) {
        val (node,depth)=pending.removeLast()
        if(++count>2_000 || depth>32)return null
        node.children().forEach {pending.add(it to depth+1)}
    }
    return root
}

internal fun markdownWebUrl(value:String):String? = runCatching {
    val uri=URI(value)
    value.takeIf {value.length<=4096 && uri.scheme?.lowercase() in setOf("http","https") && uri.host!=null && uri.rawUserInfo==null}
}.getOrNull()

internal fun markdownInline(node:Node,onLink:(String)->Unit):AnnotatedString = buildAnnotatedString {
    fun visit(value:Node) {
        when(value) {
            is MarkdownText->append(value.literal)
            is Code->withStyle(SpanStyle(fontFamily=FontFamily.Monospace)) {append(value.literal)}
            is SoftLineBreak,is HardLineBreak->append("\n")
            is StrongEmphasis->{pushStyle(SpanStyle(fontWeight=FontWeight.Bold));value.children().forEach(::visit);pop()}
            is Emphasis->{pushStyle(SpanStyle(fontStyle=FontStyle.Italic));value.children().forEach(::visit);pop()}
            is Link->{
                val url=markdownWebUrl(value.destination)
                if(url!=null)pushLink(LinkAnnotation.Clickable(url,TextLinkStyles(SpanStyle(textDecoration=TextDecoration.Underline)),{onLink(url)}))
                value.children().forEach(::visit)
                if(url!=null)pop() else append(" (${value.destination})")
            }
            is Image->{append("Image: ");value.children().forEach(::visit);append(" (${value.destination})")}
            is HtmlInline->append(value.literal)
            else->value.children().forEach(::visit)
        }
    }
    node.children().forEach(::visit)
}

@Composable fun MarkdownMessage(source:String,modifier:Modifier=Modifier) {
    // A changed source cannot display the previous message while parsing catches up.
    val parsed by produceState<Pair<String,Node?>?>(null,source) {
        delay(80)
        value=withContext(Dispatchers.Default) {source to parseMessageMarkdown(source)}
    }
    val document=parsed?.takeIf {it.first==source}?.second
    val context=LocalContext.current
    var link by remember(source) {mutableStateOf<String?>(null)}
    var linkError by remember(source) {mutableStateOf(false)}
    SelectionContainer(modifier) {
        Column(verticalArrangement=Arrangement.spacedBy(10.dp)) {
            if(document==null) {
                if(parsed?.first==source)Text("Message shown as plain text",style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                Text(source,style=MaterialTheme.typography.bodyLarge)
            } else MarkdownBlocks(document.children()) {link=it}
        }
    }
    if(linkError)Text("Link could not be opened.",style=MaterialTheme.typography.bodySmall)
    link?.let {url->AlertDialog(onDismissRequest={link=null},title={Text("Open link?")},text={Text(url)},confirmButton={TextButton({link=null;try {context.startActivity(Intent(Intent.ACTION_VIEW,url.toUri()))}catch(_:Exception) {linkError=true}}) {Text("Open")}},dismissButton={TextButton({link=null}) {Text("Cancel")}})}
}

@Composable private fun MarkdownBlocks(nodes:List<Node>,onLink:(String)->Unit) {
    nodes.forEach {node->when(node) {
        is Heading->Text(markdownInline(node,onLink),Modifier.semantics {heading()},style=when(node.level) {1->MaterialTheme.typography.headlineSmall;2->MaterialTheme.typography.titleLarge;else->MaterialTheme.typography.titleMedium})
        is MarkdownParagraph->MarkdownParagraphText(node,onLink)
        is FencedCodeBlock->MarkdownCode(node.literal,node.info)
        is IndentedCodeBlock->MarkdownCode(node.literal,"")
        is BlockQuote->Surface(color=MaterialTheme.colorScheme.surfaceContainer,shape=MaterialTheme.shapes.small) {Column(Modifier.padding(12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {MarkdownBlocks(node.children(),onLink)}}
        is BulletList,is OrderedList->{Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {node.children().forEachIndexed {index,item->
            Row {Text(if(node is OrderedList)"${node.markerStartNumber+index}. " else "• ");Column(Modifier.weight(1f),verticalArrangement=Arrangement.spacedBy(6.dp)) {MarkdownBlocks(item.children(),onLink)}}
        }}}
        is ThematicBreak->HorizontalDivider()
        is HtmlBlock->Text(node.literal,style=MaterialTheme.typography.bodyLarge)
        is TableBlock->Column(Modifier.horizontalScroll(rememberScrollState())) {
            node.children().flatMap {it.children()}.forEach {row->
                Row {row.children().forEach {cell->Surface(color=MaterialTheme.colorScheme.surfaceContainer) {Text(markdownInline(cell,onLink),Modifier.width(180.dp).padding(10.dp),fontWeight=if(cell is TableCell && cell.isHeader)FontWeight.Bold else FontWeight.Normal)}}}
                HorizontalDivider()
            }
        }
        else->MarkdownBlocks(node.children(),onLink)
    }}
}

@Composable private fun MarkdownParagraphText(node:MarkdownParagraph,onLink:(String)->Unit) {
    val text=markdownInline(node,onLink)
    val links=text.getLinkAnnotations(0,text.length)
    val actions=links.mapNotNull {range->
        val url=(range.item as? LinkAnnotation.Clickable)?.tag ?: return@mapNotNull null
        CustomAccessibilityAction("Review link: ${text.subSequence(range.start,range.end)}") {onLink(url);true}
    }
    var modifier=if(links.isEmpty())Modifier else Modifier.semantics(mergeDescendants=true) {
        contentDescription=text.text
        customActions=actions
    }
    // Give a standalone link an ordinary focusable action. Inline links remain
    // individually reviewable through the paragraph's native accessibility actions.
    links.singleOrNull()?.takeIf {it.start==0 && it.end==text.length}?.let {range->
        val url=(range.item as? LinkAnnotation.Clickable)?.tag
        if(url!=null)modifier=modifier.clickable(onClickLabel="Review link",role=Role.Button) {onLink(url)}
    }
    Text(text,modifier,style=MaterialTheme.typography.bodyLarge)
}

@Composable private fun MarkdownCode(code:String,language:String) {
    Surface(color=MaterialTheme.colorScheme.surfaceContainer,shape=MaterialTheme.shapes.small) {
        Column(Modifier.fillMaxWidth().padding(12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            if(language.isNotBlank())Text(language,style=MaterialTheme.typography.labelSmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            Text(code,Modifier.horizontalScroll(rememberScrollState()),fontFamily=FontFamily.Monospace,style=MaterialTheme.typography.bodyMedium)
        }
    }
}
