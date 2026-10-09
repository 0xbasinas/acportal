param(
    [Parameter(Mandatory=$true)][string]$AdbPath,
    [string]$Serial='emulator-5554',
    [switch]$Prepared,
    [switch]$MainActivity,
    [ValidateSet('Mcp','OfflineConversation','LiveConversation')][string]$Scenario='Mcp',
    [int]$FixturePort=0,
    [ValidateSet('None','count','bytes')][string]$Overflow='None'
)
$ErrorActionPreference='Stop'
$taskComponent=if($MainActivity){'dev.acportal/dev.acportal.MainActivity'}else{'dev.acportal/dev.acportal.presentation.TaskRecoveryFixtureActivity'}
$taskActivityPattern=if($MainActivity){'dev\.acportal/\.MainActivity'}else{'TaskRecoveryFixtureActivity'}
$taskRunner='dev.acportal.test/androidx.test.runner.AndroidJUnitRunner'
$taskXmlPath='/sdcard/acportal-task-recovery.xml'

function Invoke-TaskAdb([string[]]$Arguments) {
    $taskResult=& $AdbPath -s $Serial @Arguments 2>&1
    if ($LASTEXITCODE -ne 0) {throw ($taskResult -join "`n")}
    return ($taskResult -join "`n")
}
function Read-TaskProperties([string]$Name) {
    $taskValue=Invoke-TaskAdb @('shell','run-as','dev.acportal','cat',"cache/task-recovery-fixture/$Name")
    $taskProperties=@{}
    foreach ($taskLine in ($taskValue -split "`n")) {
        if ($taskLine -match '^([^#=]+)=(.*)$') {$taskProperties[$matches[1]]=$matches[2].Trim()}
    }
    return $taskProperties
}
function Assert-TaskForeground {
    $taskActivities=Invoke-TaskAdb @('shell','dumpsys','activity','activities')
    if ($taskActivities -notmatch "topResumedActivity=[^\r\n]*$taskActivityPattern") {throw 'Owned fixture is not foreground'}
}
function Read-TaskUi {
    Assert-TaskForeground
    for($taskDumpTry=0;$taskDumpTry -lt 4;$taskDumpTry++) {
        $taskDump=Invoke-TaskAdb @('shell','uiautomator','dump',$taskXmlPath)
        if($taskDump -match 'UI hierchary dumped to:') {return [xml](Invoke-TaskAdb @('shell','cat',$taskXmlPath))}
        Start-Sleep -Milliseconds 250
    }
    throw 'Owned fixture did not produce an idle UI hierarchy'
}
function Find-TaskNode([xml]$Document,[string]$Text,[string]$Description) {
    return @($Document.SelectNodes('//node') | Where-Object {
        ($Text -and $_.GetAttribute('text') -eq $Text) -or ($Description -and $_.GetAttribute('content-desc') -eq $Description)
    }) | Select-Object -First 1
}
function Wait-TaskNode([string]$Text='', [string]$Description='') {
    for ($taskTry=0;$taskTry -lt 8;$taskTry++) {
        $taskDocument=Read-TaskUi
        $taskNode=Find-TaskNode $taskDocument $Text $Description
        if ($taskNode) {return $taskNode}
        Start-Sleep -Milliseconds 250
    }
    throw "Fixture control not found: $Text $Description"
}
function Tap-TaskNode([string]$Text='', [string]$Description='') {
    $taskNode=Wait-TaskNode $Text $Description
    if ($taskNode.GetAttribute('bounds') -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$') {throw 'Missing fixture control bounds'}
    $taskX=[int](([int]$matches[1]+[int]$matches[3])/2)
    $taskY=[int](([int]$matches[2]+[int]$matches[4])/2)
    $null=Invoke-TaskAdb @('shell','input','tap',"$taskX","$taskY")
}
function Scroll-TaskControl([string]$Text='',[string]$Description='', [switch]$Earlier) {
    $taskDimensions=Invoke-TaskAdb @('shell','wm','size')
    if($taskDimensions -notmatch 'Physical size: (\d+)x(\d+)') {throw 'Verify emulator dimensions'}
    $taskMiddle=[int]([int]$matches[1]/2);$taskTop=[int]([int]$matches[2]*0.40);$taskBottom=[int]([int]$matches[2]*0.65)
    for($taskScroll=0;$taskScroll -lt 16;$taskScroll++) {
        $taskNode=Find-TaskNode (Read-TaskUi) $Text $Description
        if($taskNode) {return $taskNode}
        $taskFrom=if($Earlier){$taskTop}else{$taskBottom};$taskTo=if($Earlier){$taskBottom}else{$taskTop}
        $null=Invoke-TaskAdb @('shell','input','swipe',"$taskMiddle","$taskFrom","$taskMiddle","$taskTo",'400')
    }
    throw 'Owned timeline control did not become reachable'
}
function Run-TaskTest([string]$Method) {
    $taskResult=Invoke-TaskAdb @('shell','am','instrument','-w','-r','-e','fixtureOverflow',$Overflow,'-e','fixturePort',"$FixturePort",'-e','taskScenario',$Scenario,'-e','class',"dev.acportal.presentation.TaskRecoveryNavigationTest#$Method",$taskRunner)
    if ($taskResult -notmatch 'OK \(1 test\)') {throw $taskResult}
    Write-Output "Passed $Method"
}
function Read-RecoveryHost {return Invoke-RestMethod "http://127.0.0.1:$FixturePort/fixture/status"}
function Set-RecoveryGate([string]$Action) {$null=Invoke-RestMethod -Method Post "http://127.0.0.1:$FixturePort/fixture/$Action"}
function Assert-NoRecoveryMutations {if((Read-RecoveryHost).mutations -ne 0) {throw 'Recovery automatically sent a mutation'}}

# Install the current app/test APKs before invoking this workflow. No instrumentation
# runs between launching the owned fixture and checking restored task navigation.
if($Scenario -eq 'LiveConversation') {
    if($FixturePort -lt 1024 -or $FixturePort -gt 65535) {throw 'Supply owned recovery-host.py port'}
    $taskHost=Read-RecoveryHost
    if($taskHost.mutations -ne 0 -or $taskHost.connections.'live-1' -ne 0 -or $taskHost.connections.'live-2' -ne 0) {throw 'Use a fresh owned recovery host'}
    $null=Invoke-TaskAdb @('reverse',"tcp:$FixturePort","tcp:$FixturePort")
}
if (!$Prepared) {Run-TaskTest 'prepareOwnedStorageForExternalTaskWorkflow'}
if ($MainActivity) {
    $null=Invoke-TaskAdb @('shell','run-as','dev.acportal','cp','cache/task-recovery-fixture/enabled','cache/task-recovery-fixture/main-enabled')
    # The marker must be read by a new Application before MainActivity launches.
    $null=Invoke-TaskAdb @('shell','am','force-stop','dev.acportal')
}
try {
    $null=Invoke-TaskAdb @('shell','am','start','-W','-n',$taskComponent,'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER','-f','0x10000000')
    if($Scenario -eq 'LiveConversation') {
        Tap-TaskNode -Text 'Live 2 prompt'
        $null=Wait-TaskNode -Text 'Original live-2 approval'
        $null=Invoke-TaskAdb @('shell','input','keyevent','4')
        Tap-TaskNode -Description 'Back'
        Tap-TaskNode -Text 'Live 1 prompt'
        $null=Wait-TaskNode -Text 'Original live-1 approval'
        $null=Invoke-TaskAdb @('shell','input','keyevent','4')
        Tap-TaskNode -Description 'Expand tool: Retained tool 1'
        $null=Wait-TaskNode -Text 'Retained tool output 1'
        $null=Scroll-TaskControl -Description 'Show agent thoughts'
        Tap-TaskNode -Description 'Show agent thoughts'
        $null=Scroll-TaskControl -Text 'Retained thought 1'
        $taskAnchorText='Live 1 history 18. '+('Retained fixture history. '*8)
        $taskAnchor=Scroll-TaskControl -Text $taskAnchorText -Earlier
        $taskAnchorBounds=$taskAnchor.GetAttribute('bounds')
        Assert-NoRecoveryMutations
        Set-RecoveryGate 'hold'
    } elseif($Scenario -eq 'OfflineConversation') {
        Tap-TaskNode -Text 'Offline 1 prompt'
        Tap-TaskNode -Text 'View saved conversation'
        $null=Wait-TaskNode -Text 'Offline 1 retained answer'
        $taskUi=Read-TaskUi
        if($taskUi.OuterXml -match 'Do not approve stale request|Stale offline approval') {throw 'Offline copy exposes stale permission controls'}
    } else {
    Tap-TaskNode -Description 'Settings tab'
    Tap-TaskNode -Text 'MCP servers'
    Tap-TaskNode -Text 'Task workstation'
    Tap-TaskNode -Text 'Task saved server'
    Tap-TaskNode -Text 'Task saved server'
    $null=Invoke-TaskAdb @('shell','input','text','unsaved-task-value')
    $taskDraft=Read-TaskUi
    if ($taskDraft.OuterXml -notmatch 'unsaved-task-value') {throw 'Unpersisted editor draft was not entered'}
    $null=Invoke-TaskAdb @('shell','input','keyevent','4')
    $taskBeforeBackground=Read-TaskUi
    if ($taskBeforeBackground.OuterXml -notmatch 'unsaved-task-value') {throw 'Keyboard dismissal unexpectedly discarded the editor draft'}
    }
    $taskPrevious=Read-TaskProperties 'created.properties'
    if ($taskPrevious.savedState -ne 'false') {throw 'Preparation unexpectedly restored old state'}
    $null=Invoke-TaskAdb @('shell','run-as','dev.acportal','cp','cache/task-recovery-fixture/created.properties','cache/task-recovery-fixture/checkpoint.properties')
    $null=Invoke-TaskAdb @('shell','input','keyevent','3')
    Start-Sleep -Milliseconds 500
    $taskSaved=Read-TaskProperties 'saved.properties'
    if ($taskSaved.pid -ne $taskPrevious.pid -or $taskSaved.task -ne $taskPrevious.task) {throw 'Framework save identity does not match'}
    $taskActivities=Invoke-TaskAdb @('shell','dumpsys','activity','activities')
    if ($taskActivities -match 'topResumedActivity=[^\r\n]*dev\.acportal') {throw 'App must be background before termination'}
    if ($taskActivities -notmatch $taskActivityPattern) {throw 'Previous fixture task is missing'}
    $taskLive=(Invoke-TaskAdb @('shell','pidof','dev.acportal')).Trim()
    if ($taskLive -ne $taskPrevious.pid -or $taskLive -notmatch '^\d+$') {throw 'Live process does not match fixture identity'}
    $null=Invoke-TaskAdb @('shell','run-as','dev.acportal','kill','-9',$taskLive)
    Start-Sleep -Milliseconds 500
    $taskRemaining=& $AdbPath -s $Serial shell pidof dev.acportal
    if ($taskRemaining) {throw 'App process remained alive'}
    $null=Invoke-TaskAdb @('shell','am','start','-W','-n',$taskComponent,'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER','-f','0x10000000')
    if($Scenario -eq 'LiveConversation') {
        # The HTTP session response is still held. Check local restoration before
        # either metadata or WebSocket replay can replace cached presentation.
        $null=Wait-TaskNode -Text 'Saved conversation'
        $null=Wait-TaskNode -Text $taskAnchorText
        $taskLocalUi=Read-TaskUi
        if($taskLocalUi.OuterXml -match 'Original live-1 approval|Deny fixture request|Fresh authoritative request') {throw 'Local restored copy exposed approval controls before metadata'}
        Assert-NoRecoveryMutations
        Set-RecoveryGate 'metadata'
        $null=Wait-TaskNode -Text 'Restoring conversation'
        $taskHeldUi=Read-TaskUi
        if($taskHeldUi.OuterXml -match 'Original live-1 approval|Deny fixture request|Fresh authoritative request') {throw 'Approval exposed before authoritative replay'}
        $taskSend=Find-TaskNode $taskHeldUi '' 'Send message'
        while($taskSend -and $taskSend.GetAttribute('clickable') -ne 'true' -and $taskSend.ParentNode.Name -eq 'node') {$taskSend=$taskSend.ParentNode}
        if($taskSend -and $taskSend.GetAttribute('enabled') -eq 'true') {throw 'Prompt enabled during replay'}
        $taskStop=Find-TaskNode $taskHeldUi '' 'Stop turn'
        while($taskStop -and $taskStop.GetAttribute('clickable') -ne 'true' -and $taskStop.ParentNode.Name -eq 'node') {$taskStop=$taskStop.ParentNode}
        if($taskStop -and $taskStop.GetAttribute('enabled') -eq 'true') {throw 'Cancel enabled before active-turn replay completed'}
        $taskRestoredAnchor=Wait-TaskNode -Text $taskAnchorText
        if($taskRestoredAnchor.GetAttribute('bounds') -ne $taskAnchorBounds) {throw 'Restored live scroll anchor moved before any scroll action'}
        Assert-NoRecoveryMutations
        Set-RecoveryGate 'release'
        $null=Wait-TaskNode -Text 'Fresh authoritative request'
    } elseif($Scenario -eq 'OfflineConversation') {$null=Wait-TaskNode -Text 'Offline 1 retained answer'}
    else {$null=Wait-TaskNode -Text 'Task saved server'}
    $taskRestored=Read-TaskProperties 'created.properties'
    if ($taskRestored.pid -eq $taskPrevious.pid -or $taskRestored.task -ne $taskPrevious.task -or $taskRestored.savedState -ne 'true') {throw 'Previous task/framework state was not restored in a distinct process'}
    $taskRestoredUi=Read-TaskUi
    if($Scenario -eq 'LiveConversation') {
        if($taskRestoredUi.OuterXml -match 'Original live-1 approval') {throw 'Stale approval restored'}
        Assert-NoRecoveryMutations
        Tap-TaskNode -Text 'Deny fixture request'
        Start-Sleep -Milliseconds 750
        $taskHost=Read-RecoveryHost
        if($taskHost.mutations -ne 1 -or $taskHost.decisions.Count -ne 1 -or $taskHost.decisions[0].id -ne 'fresh-1' -or $taskHost.decisions[0].session -ne 'live-1') {throw 'Explicit decision was not independently scoped'}
        $null=Scroll-TaskControl -Text 'Retained tool output 1'
        $null=Scroll-TaskControl -Text 'Retained thought 1'
        Tap-TaskNode -Description 'Back'
        Tap-TaskNode -Text 'Live 2 prompt'
        $null=Wait-TaskNode -Text 'Live 2 retained answer'
        $taskSecond=Read-TaskUi
        if($taskSecond.OuterXml -match 'Original live-2 approval|Fresh authoritative request|Deny fixture request|Live 1 retained answer') {throw 'Second session retained stale approvals or another session state'}
        if((Read-RecoveryHost).mutations -ne 1) {throw 'Second session recovery sent a mutation'}
        if($Overflow -ne 'None') {
            Set-RecoveryGate "overflow-$Overflow"
            $null=Wait-TaskNode -Text 'Overflow fixture approval'
            $null=Invoke-TaskAdb @('shell','input','keyevent','4')
            $null=Wait-TaskNode -Text 'Connection lost'
            Tap-TaskNode -Text 'Dismiss'
            $null=Wait-TaskNode -Text 'Reconnect'
            Tap-TaskNode -Text 'Review permission'
            $taskDisabled=Wait-TaskNode -Text 'Deny fixture request'
            while($taskDisabled -and $taskDisabled.GetAttribute('clickable') -ne 'true' -and $taskDisabled.ParentNode.Name -eq 'node') {$taskDisabled=$taskDisabled.ParentNode}
            if($taskDisabled.GetAttribute('enabled') -ne 'false') {throw 'Overflow decision re-enabled after dismissal'}
            $null=Invoke-TaskAdb @('shell','input','keyevent','4')
            if((Read-RecoveryHost).mutations -ne 1) {throw 'Overflow sent a decision or repeated prior consent'}
        }
        Start-Sleep -Milliseconds 750
        Write-Output "Verified task $($taskRestored.task), distinct process, held active-turn replay, replaced/removed approvals, explicit scoped decision, two live sessions, exact scroll anchor and offscreen tool/thought expansions; overflow=$Overflow."
    } elseif($Scenario -eq 'OfflineConversation') {
        if($taskRestoredUi.OuterXml -match 'Do not approve stale request|Stale offline approval') {throw 'Restored offline copy exposes stale approvals'}
        Tap-TaskNode -Description 'Back'
        $null=Wait-TaskNode -Text 'View saved conversation'
        Tap-TaskNode -Description 'Back'
        $null=Wait-TaskNode -Description 'Sessions tab'
        Tap-TaskNode -Text 'Offline 2 prompt'
        Tap-TaskNode -Text 'View saved conversation'
        $null=Wait-TaskNode -Text 'Offline 2 retained answer'
        Tap-TaskNode -Description 'Back'
        Tap-TaskNode -Description 'Back'
        $null=Wait-TaskNode -Text 'Offline 1 prompt'
        $null=Wait-TaskNode -Text 'Offline 2 prompt'
        Write-Output "Verified task $($taskRestored.task), distinct PID, restored offline history, guarded stale approvals and two-session Back navigation."
    } else {
    if ($taskRestoredUi.OuterXml -match 'unsaved-task-value|Server name') {throw 'Unsaved editor draft was restored'}
    Tap-TaskNode -Description 'Back'
    $null=Wait-TaskNode -Text 'Task workstation'
    Tap-TaskNode -Description 'Back'
    $null=Wait-TaskNode -Description 'Settings tab'
    $null=Wait-TaskNode -Text 'Appearance'
    $null=Wait-TaskNode -Text 'History on this device'
    Write-Output "Verified task $($taskRestored.task), distinct PID, restored MCP route, discarded draft and Back to chooser/Settings."
    }
    if($MainActivity) {$null=Invoke-TaskAdb @('shell','am','force-stop','dev.acportal')}
    else {$null=Invoke-TaskAdb @('shell','am','start','-n',$taskComponent,'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER','-f','0x30000000','--ez','finish_fixture','true')}
    Run-TaskTest 'verifyRecordedTaskRestorationAndCleanOwnedStorage'
} finally {
    $null=Invoke-TaskAdb @('shell','rm','-f',$taskXmlPath)
    if($Scenario -eq 'LiveConversation') {$null=Invoke-TaskAdb @('reverse','--remove',"tcp:$FixturePort")}
}
