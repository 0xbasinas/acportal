package dev.acportal.protocol

import java.io.ByteArrayOutputStream
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.*

/** Stops encoding at the byte boundary, before a full JSON String/UTF-8 copy exists. */
@OptIn(ExperimentalSerializationApi::class)
fun boundedJsonBytes(value:JsonElement,limit:Int,message:String):ByteArray {
    require(limit>0)
    val output=object:ByteArrayOutputStream(minOf(limit,8192)) {
        override fun write(value:Int) {require(count<limit) {message};super.write(value)}
        override fun write(bytes:ByteArray,offset:Int,length:Int) {require(length<=limit-count) {message};super.write(bytes,offset,length)}
    }
    WireJson.encodeToStream(JsonElement.serializer(),value,output)
    return output.toByteArray()
}
