package dev.acportal.data

import dev.acportal.protocol.*
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.*
import okhttp3.HttpUrl.Companion.toHttpUrl

@Serializable data class McpValue(val name:String,val value:String)
@Serializable data class McpDefinition(val id:String,val name:String,val transport:String="stdio",val endpoint:String,val args:List<String> = emptyList(),val values:List<McpValue> = emptyList(),val enabled:Boolean=true) {
    fun wire():JsonObject {
        require(name.isNotBlank() && name.length<=128 && name.none(Char::isISOControl)) {"Enter a server name of up to 128 characters"}
        require(transport in listOf("stdio","http","sse")) {"Choose a supported server type"}
        require(values.size<=32 && values.map {it.name.lowercase()}.distinct().size==values.size) {"Use up to 32 unique environment variables or headers"}
        values.forEach {entry->
            val pattern=if(transport=="stdio")Regex("[A-Za-z0-9_]+") else Regex("[A-Za-z0-9!#$%&'*+.^_`|~-]+")
            require(entry.name.length<=128 && pattern.matches(entry.name) && entry.value.length<=4096 && '\u0000' !in entry.value && (transport=="stdio" || entry.value.none {it=='\r' || it=='\n'})) {"Check the environment variable or header names and values"}
        }
        if(transport=="stdio") {
            require(endpoint.length<=4096 && '\u0000' !in endpoint && (endpoint.startsWith('/') || endpoint.startsWith("\\\\") || Regex("^[A-Za-z]:[\\\\/].*").matches(endpoint))) {"Enter an absolute executable path on this host"}
            require(args.size<=128 && args.all {it.length<=4096 && '\u0000' !in it}) {"Use up to 128 arguments without null characters"}
        } else {
            val url=endpoint.toHttpUrl()
            require(url.username.isEmpty() && url.password.isEmpty() && url.fragment==null) {"Enter an HTTP or HTTPS URL without embedded credentials or a fragment"}
        }
        return buildJsonObject {
            put("name",name)
            if(transport=="stdio") {put("command",endpoint);putJsonArray("args") {args.forEach {add(it)}}}
            else {put("type",transport);put("url",endpoint)}
            putJsonArray(if(transport=="stdio")"env" else "headers") {values.forEach {entry->add(buildJsonObject {put("name",entry.name);put("value",entry.value)})}}
        }
    }
    fun displayEndpoint():String = if(transport=="stdio")"Runs on your connected PC" else runCatching {endpoint.toHttpUrl().let {"${it.host}${it.encodedPath}"}}.getOrDefault("HTTP server")
}

fun mcpWire(servers:List<McpDefinition>):JsonArray {
    require(servers.size<=16 && servers.map {it.name}.distinct().size==servers.size) {"Use up to 16 servers with unique names"}
    servers.forEach {it.wire()}
    val wire=JsonArray(servers.filter {it.enabled}.map {it.wire()})
    require(wire.toString().toByteArray().size<=12000) {"Server definitions exceed the host's size limit"}
    return wire
}

fun parseMcpValues(text:String):List<McpValue> = text.lineSequence().filter {it.isNotBlank()}.map {line->
    val separator=line.indexOf('=')
    require(separator>0) {"Enter one NAME=value per line"}
    McpValue(line.substring(0,separator).trim(),line.substring(separator+1))
}.toList()
