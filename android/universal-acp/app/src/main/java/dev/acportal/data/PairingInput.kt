package dev.acportal.data

import java.net.URI
import java.net.URLDecoder
import java.util.Locale

data class PairingInput(val address:String,val code:String,val name:String)

fun normalizePairingCode(value:String):String {
    val compact=value.filterNot {it=='-' || it.isWhitespace()}.uppercase(Locale.ROOT)
    return if(compact.length==12 && compact.all {it in '0'..'9' || it in 'A'..'F'})compact.chunked(4).joinToString("-") else value.trim().uppercase(Locale.ROOT)
}

/** An import fills a reviewable form, never redeems a code or relaxes address validation. */
fun parsePairingInput(value:String,debug:Boolean):PairingInput {
    require(value.length<=4096) {"Pairing QR code is too large."}
    val uri=runCatching {URI(value)}.getOrNull()
    require(uri!=null && uri.scheme=="acportal" && uri.rawAuthority=="pair" && uri.rawPath.isNullOrEmpty() && uri.rawFragment==null) {"Scan an ACP Portal pairing QR code."}
    val values=linkedMapOf<String,String>()
    uri.rawQuery.orEmpty().split('&').forEach {part->
        val pieces=part.split('=',limit=2)
        require(pieces.size==2) {"Pairing QR code is invalid."}
        val key=runCatching {URLDecoder.decode(pieces[0],"UTF-8")}.getOrNull()
        require(key in setOf("address","code","name") && key !in values) {"Pairing QR code is invalid."}
        val decoded=runCatching {URLDecoder.decode(pieces[1],"UTF-8")}.getOrNull()
        require(decoded!=null) {"Pairing QR code is invalid."}
        values[requireNotNull(key)]=decoded
    }
    val address=values["address"].orEmpty()
    // Never surface exceptions containing the imported one-use code.
    val validated=runCatching {validateAddress(address,debug).toString().trimEnd('/')}.getOrNull()
    require(validated!=null) {"The QR code needs a trusted HTTPS host address."}
    val code=normalizePairingCode(values["code"].orEmpty())
    require(Regex("[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}").matches(code)) {"Pairing QR code has an invalid code."}
    val name=values["name"].orEmpty()
    require(name.length<=120 && name.none {it.isISOControl()}) {"Pairing QR code has an invalid connection name."}
    return PairingInput(validated,code,name)
}
