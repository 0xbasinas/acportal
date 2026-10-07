package dev.acportal.presentation

import dev.acportal.protocol.*
import dev.acportal.storage.StoredSession
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.temporal.ChronoUnit
import java.util.Locale

data class SessionActivityTime(val timestamp:Long,val reportedByAgent:Boolean)
fun sessionActivityTime(stored:StoredSession,state:SessionState):SessionActivityTime? {
    val reported=runCatching {Instant.parse(state.sessionInfo["updatedAt"].text()).toEpochMilli()}.getOrNull()
    return if(reported!=null)SessionActivityTime(reported,true) else stored.updatedAt.takeIf {it>0}?.let {SessionActivityTime(it,false)}
}
fun activityTimeLabel(timestamp:Long,now:Long,zone:ZoneId=ZoneId.systemDefault(),locale:Locale=Locale.getDefault()):String {
    val instant=Instant.ofEpochMilli(timestamp)
    val minutes=((now-timestamp)/60_000).coerceAtLeast(0)
    val days=ChronoUnit.DAYS.between(instant.atZone(zone).toLocalDate(),Instant.ofEpochMilli(now).atZone(zone).toLocalDate())
    if(timestamp<=now+60_000) {
        if(minutes<1)return "Just now"
        if(days==0L && minutes<60)return "$minutes ${if(minutes==1L)"minute" else "minutes"} ago"
        if(days==0L) {val hours=minutes/60;return "$hours ${if(hours==1L)"hour" else "hours"} ago"}
        if(days==1L)return "Yesterday, ${DateTimeFormatter.ofLocalizedTime(FormatStyle.SHORT).withLocale(locale).withZone(zone).format(instant)}"
    }
    return DateTimeFormatter.ofLocalizedDateTime(FormatStyle.SHORT).withLocale(locale).withZone(zone).format(instant)
}

