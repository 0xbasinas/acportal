package dev.acportal.presentation

import androidx.compose.foundation.layout.*
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.*
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import dev.acportal.data.InstalledBuild
import dev.acportal.data.ReleaseInfo
import dev.acportal.data.UpdateChannel
import dev.acportal.data.UpdateSource
import dev.acportal.data.UpdateState
import java.time.Instant
import java.time.ZoneId
import java.time.format.DateTimeFormatter
import java.util.Locale

private val ButtonShape=RoundedCornerShape(12.dp)

/** "54 MB", or one decimal below 10 MB. */
internal fun formatMegabytes(bytes:Long):String {
    val megabytes=bytes/(1024.0*1024.0)
    return if(megabytes>=10)"${Math.round(megabytes)} MB" else String.format(Locale.US,"%.1f MB",megabytes)
}

/** GitHub's ISO time as local "11 Oct 2026, 18:02"; the text unchanged if it cannot be read. */
internal fun formatPublished(iso:String?,zone:ZoneId=ZoneId.systemDefault()):String? {
    if(iso.isNullOrBlank())return null
    return try {DateTimeFormatter.ofPattern("d MMM yyyy, HH:mm",Locale.ENGLISH).format(Instant.parse(iso).atZone(zone))} catch(failure:Exception) {iso}
}

/**
 * Settings > Update app. Checks the project's GitHub release, downloads the newer build with its
 * checksum verified, and hands it to Android's installer, which verifies the signature and asks
 * the user to confirm. Nothing is downloaded or installed until the user presses the button.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable fun UpdateScreen(state:UpdateState,installed:InstalledBuild,onBack:()->Unit,onCheck:()->Unit,onInstall:(ReleaseInfo)->Unit,onCancel:()->Unit,onOpenSettings:()->Unit,channel:UpdateChannel=UpdateChannel.installed(installed.versionName),onChannel:(UpdateChannel)->Unit={}) {
    val installedChannel=UpdateChannel.installed(installed.versionName)
    val busy=state is UpdateState.Downloading || state is UpdateState.Installing
    Column(Modifier.fillMaxSize()) {
        ScreenHeader("Update app","From GitHub releases",onBack)
        Column(Modifier.weight(1f).testTag("update-body").verticalScroll(rememberScrollState()).padding(horizontal=24.dp,vertical=8.dp),verticalArrangement=Arrangement.spacedBy(16.dp)) {
            InfoCard {
                Text("Installed",style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                Text(installed.versionName.ifBlank {"Unknown"},style=MaterialTheme.typography.titleMedium)
                Text("Build ${installed.build} · ${installedChannel.singular}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
                Text("Follow",style=MaterialTheme.typography.labelMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    UpdateChannel.entries.forEachIndexed {index,option->
                        SegmentedButton(channel==option,{onChannel(option)},SegmentedButtonDefaults.itemShape(index,UpdateChannel.entries.size),enabled=!busy,modifier=Modifier.heightIn(min=40.dp).testTag("update-channel-${option.name.lowercase()}")) {Text(option.label)}
                    }
                }
                if(channel!=installedChannel) {
                    Text("Signed releases and debug builds use different signing keys, so Android will not install one over the other. To switch, uninstall the app once (saved connections are removed), then install the build you want.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.error,modifier=Modifier.testTag("update-switch-note"))
                }
            }
            when(state) {
                UpdateState.Idle,UpdateState.Checking->Column(verticalArrangement=Arrangement.spacedBy(8.dp)) {
                    Text("Checking for updates…",style=MaterialTheme.typography.titleSmall)
                    LinearProgressIndicator(Modifier.fillMaxWidth())
                }
                is UpdateState.UpToDate->Column(verticalArrangement=Arrangement.spacedBy(4.dp)) {
                    Text("You're up to date",style=MaterialTheme.typography.titleMedium,color=MaterialTheme.colorScheme.primary,modifier=Modifier.testTag("update-status"))
                    Text(if(channel==UpdateChannel.Release)"The latest signed release is ${state.latest.versionName}." else "The latest release is build ${state.latest.build}.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                is UpdateState.Available->{ReleaseCard(state.release,"Update available");TrustNote(state.release.sha256!=null)}
                is UpdateState.Downloading->InfoCard {
                    Text("Downloading build ${state.release.build}",style=MaterialTheme.typography.titleSmall)
                    if(state.release.sizeBytes>0) {
                        LinearProgressIndicator({(state.done.toFloat()/state.release.sizeBytes).coerceIn(0f,1f)},Modifier.fillMaxWidth().testTag("update-progress"))
                        Text("${formatMegabytes(state.done)} of ${formatMegabytes(state.release.sizeBytes)}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                    } else LinearProgressIndicator(Modifier.fillMaxWidth())
                    Text("Keep this screen open until the install prompt appears.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                is UpdateState.NeedsPermission->InfoCard {
                    Text("Allow installs from this app",style=MaterialTheme.typography.titleSmall,modifier=Modifier.testTag("update-status"))
                    Text("Android asks you to allow ACP Portal to install apps once. Turn on \"Allow from this source\", then come back and press Download and install again.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                is UpdateState.Installing->InfoCard {
                    Text("Waiting for Android",style=MaterialTheme.typography.titleSmall,modifier=Modifier.testTag("update-status"))
                    Text("Confirm the install when Android asks. The app restarts when it is done and your data is kept.",style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
                is UpdateState.Failed->Column(verticalArrangement=Arrangement.spacedBy(4.dp)) {
                    Text(if(state.release==null)"Could not check for updates" else "The update did not finish",style=MaterialTheme.typography.titleMedium,color=MaterialTheme.colorScheme.error,modifier=Modifier.testTag("update-status"))
                    Text(state.message,style=MaterialTheme.typography.bodyMedium,color=MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
        Column(Modifier.padding(horizontal=24.dp,vertical=12.dp),verticalArrangement=Arrangement.spacedBy(8.dp)) {
            when(state) {
                is UpdateState.Available->{
                    Button({onInstall(state.release)},Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-primary"),shape=ButtonShape) {Text("Download and install")}
                    OutlinedButton(onCheck,Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-secondary"),shape=ButtonShape) {Text("Check again")}
                }
                is UpdateState.UpToDate->OutlinedButton(onCheck,Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-secondary"),shape=ButtonShape) {Text("Check again")}
                is UpdateState.Downloading->OutlinedButton(onCancel,Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-secondary"),shape=ButtonShape) {Text("Cancel")}
                is UpdateState.NeedsPermission->Button(onOpenSettings,Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-primary"),shape=ButtonShape) {Text("Open settings")}
                is UpdateState.Failed->Button({state.release?.let(onInstall) ?: onCheck()},Modifier.fillMaxWidth().heightIn(min=44.dp).testTag("update-primary"),shape=ButtonShape) {Text("Try again")}
                UpdateState.Idle,UpdateState.Checking,is UpdateState.Installing->Unit
            }
        }
    }
}

@Composable private fun InfoCard(content:@Composable ColumnScope.()->Unit) {
    Surface(shape=MaterialTheme.shapes.medium,color=MaterialTheme.colorScheme.surfaceContainer,modifier=Modifier.fillMaxWidth()) {
        Column(Modifier.padding(16.dp),verticalArrangement=Arrangement.spacedBy(6.dp),content=content)
    }
}

@Composable private fun ReleaseCard(release:ReleaseInfo,title:String) {
    InfoCard {
        Text(title,style=MaterialTheme.typography.titleSmall,color=MaterialTheme.colorScheme.primary,modifier=Modifier.testTag("update-status"))
        Text(release.versionName,style=MaterialTheme.typography.titleMedium)
        Text(listOfNotNull("Build ${release.build}",release.sizeBytes.takeIf {it>0}?.let(::formatMegabytes),formatPublished(release.publishedAt)?.let {"Published $it"}).joinToString(" · "),style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
        Text("From github.com/${UpdateSource.REPOSITORY}",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable private fun TrustNote(hasChecksum:Boolean) {
    Text(if(hasChecksum)"The download is checked against the release checksum, and Android only accepts it if it is signed with the same key as this app. Your data is kept." else "This release has no checksum, so only Android's check applies: it accepts the update only if it is signed with the same key as this app. Your data is kept.",style=MaterialTheme.typography.bodySmall,color=MaterialTheme.colorScheme.onSurfaceVariant)
}
