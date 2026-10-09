param(
    [Parameter(Mandatory=$true)][string]$AdbPath,
    [string]$Serial='emulator-5554',
    [switch]$Prepared,
    [switch]$MainActivity,
    [ValidateSet('Mcp','OfflineConversation')][string]$Scenario='Mcp'
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
    $null=Invoke-TaskAdb @('shell','uiautomator','dump',$taskXmlPath)
    return [xml](Invoke-TaskAdb @('shell','cat',$taskXmlPath))
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
function Run-TaskTest([string]$Method) {
    $taskResult=Invoke-TaskAdb @('shell','am','instrument','-w','-r','-e','taskScenario',$Scenario,'-e','class',"dev.acportal.presentation.TaskRecoveryNavigationTest#$Method",$taskRunner)
    if ($taskResult -notmatch 'OK \(1 test\)') {throw $taskResult}
    Write-Output "Passed $Method"
}

# Install the current app/test APKs before invoking this workflow. No instrumentation
# runs between launching the owned fixture and checking restored task navigation.
if (!$Prepared) {Run-TaskTest 'prepareOwnedStorageForExternalTaskWorkflow'}
if ($MainActivity) {
    $null=Invoke-TaskAdb @('shell','run-as','dev.acportal','cp','cache/task-recovery-fixture/enabled','cache/task-recovery-fixture/main-enabled')
    # The marker must be read by a new Application before MainActivity launches.
    $null=Invoke-TaskAdb @('shell','am','force-stop','dev.acportal')
}
try {
    $null=Invoke-TaskAdb @('shell','am','start','-W','-n',$taskComponent,'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER','-f','0x10000000')
    if($Scenario -eq 'OfflineConversation') {
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
    if($Scenario -eq 'OfflineConversation') {$null=Wait-TaskNode -Text 'Offline 1 retained answer'}
    else {$null=Wait-TaskNode -Text 'Task saved server'}
    $taskRestored=Read-TaskProperties 'created.properties'
    if ($taskRestored.pid -eq $taskPrevious.pid -or $taskRestored.task -ne $taskPrevious.task -or $taskRestored.savedState -ne 'true') {throw 'Previous task/framework state was not restored in a distinct process'}
    $taskRestoredUi=Read-TaskUi
    if($Scenario -eq 'OfflineConversation') {
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
}
