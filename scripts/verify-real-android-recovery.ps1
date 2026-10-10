param([Parameter(Mandatory=$true)][string]$AdbPath,[string]$Serial='emulator-5554',[int]$BootstrapPort,[int]$HostPort)
$ErrorActionPreference='Stop'
$fixturePackage='dev.acportal.acceptance'
$fixtureXml='/sdcard/acportal-real-recovery.xml'
$ownedReverse=[System.Collections.Generic.HashSet[int]]::new()
function Adb([string[]]$Arguments) {
    # Windows PowerShell classifies any native stderr as an error, even on exit 0.
    $previousPreference=$ErrorActionPreference
    try {
        $ErrorActionPreference='Continue'
        $result=& $AdbPath -s $Serial @Arguments 2>&1
        $nativeExit=$LASTEXITCODE
    } finally {$ErrorActionPreference=$previousPreference}
    if($nativeExit -ne 0){throw 'Owned recovery adb operation failed'}
    return ($result -join "`n")
}
function ReadUi {
    $foreground=$false
    for($attempt=0;$attempt -lt 12;$attempt++) {
        $activities=Adb @('shell','dumpsys','activity','activities')
        if($activities -match 'topResumedActivity=[^\r\n]*dev.acportal.acceptance/'){$foreground=$true;break}
        Start-Sleep -Milliseconds 250
    }
    if(!$foreground){throw 'Owned acceptance app is not foreground'}
    for($attempt=0;$attempt -lt 4;$attempt++) {
        $dump=Adb @('shell','uiautomator','dump',$fixtureXml)
        if($dump -match 'dumped to:'){return [xml](Adb @('shell','cat',$fixtureXml))}
        Start-Sleep -Milliseconds 250
    }
    throw 'Owned recovery UI did not settle'
}
function WaitNode([string]$Text,[string]$Description='') {
    for($attempt=0;$attempt -lt 8;$attempt++) {
        $document=ReadUi
        $node=@($document.SelectNodes('//node') | Where-Object {($_.GetAttribute('text') -eq $Text -and $Text) -or ($Description -and $_.GetAttribute('content-desc') -eq $Description)}) | Select-Object -First 1
        if($node){return $node}
        Start-Sleep -Milliseconds 250
    }
    Write-Output "CHECK UI target unavailable: $Text $Description"
    throw "Owned recovery control unavailable: $Text $Description"
}
function Tap([string]$Text,[string]$Description='') {
    $node=WaitNode $Text $Description
    if($node.GetAttribute('bounds') -notmatch '^\[(\d+),(\d+)\]\[(\d+),(\d+)\]$'){throw 'Fixture bounds missing'}
    $x=[int](([int]$matches[1]+[int]$matches[3])/2);$y=[int](([int]$matches[2]+[int]$matches[4])/2)
    $null=Adb @('shell','input','tap',"$x","$y")
}
try {
    foreach($port in @($BootstrapPort,$HostPort)) {$null=Adb @('reverse',"tcp:$port","tcp:$port");$null=$ownedReverse.Add($port)}
    $prepared=Adb @('shell','am','instrument','-w','-r','-e','realBootstrapPort',"$BootstrapPort",'-e','class','dev.acportal.data.RealGooseRecoverySetupTest#prepareTwoOwnedRealSessions','dev.acportal.acceptance.test/androidx.test.runner.AndroidJUnitRunner')
    if($prepared -notmatch 'OK \(1 test\)' -or $prepared -match 'FAILURES|SKIPPED'){throw 'Real recovery preparation failed'}
    Write-Output 'CHECK Android fixture prepared'
    $bootstrap=Invoke-RestMethod -Uri "http://127.0.0.1:$BootstrapPort/bootstrap"
    $decisionLabels=@($bootstrap.permissionLabels)
    $permissionHeadings=@('Permission needed','Host permission needed')
    if($bootstrap.permissionHeading){$permissionHeadings=@([string]$bootstrap.permissionHeading)}
    $null=Adb @('shell','am','force-stop',$fixturePackage)
    $null=Adb @('shell','am','start','-W','-f','0x10008000','-n',"$fixturePackage/dev.acportal.MainActivity",'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER')
    Tap 'Real recovery 1'
    Write-Output 'CHECK real session opened on Android'
    # An actual host/agent permission must be visible before process termination.
    $permission=$null
    for($attempt=0;$attempt -lt 8;$attempt++) {
        $document=ReadUi
        $permission=@($document.SelectNodes('//node') | Where-Object {$_.GetAttribute('text') -in $permissionHeadings}) | Select-Object -First 1
        if($permission){break};Start-Sleep -Milliseconds 250
    }
    if(!$permission){throw 'Real approval did not reach Android'}
    Write-Output 'CHECK real approval visible before termination'
    $oldPid=(Adb @('shell','pidof',$fixturePackage)).Trim()
    $activities=Adb @('shell','dumpsys','activity','activities')
    if($activities -notmatch 'topResumedActivity=[^\r\n]*dev.acportal.acceptance/[^\r\n]* t(\d+)'){throw 'Owned task identity missing'}
    $taskId=$matches[1]
    $null=Adb @('shell','input','keyevent','KEYCODE_HOME')
    Start-Sleep -Seconds 2
    $background=Adb @('shell','dumpsys','activity','activities')
    if($background -match 'topResumedActivity=[^\r\n]*dev.acportal.acceptance/'){throw 'Owned app did not background'}
    $null=Adb @('shell','am','kill',$fixturePackage)
    Start-Sleep -Milliseconds 500
    $remaining=& $AdbPath -s $Serial shell pidof $fixturePackage 2>$null
    if($remaining){throw 'Owned process still alive'}
    Write-Output 'CHECK Android process absent'
    $null=Adb @('reverse','--remove',"tcp:$HostPort")
    $null=$ownedReverse.Remove($HostPort)
    $null=Adb @('shell','am','start','-W','-n',"$fixturePackage/dev.acportal.MainActivity",'-a','android.intent.action.MAIN','-c','android.intent.category.LAUNCHER')
    $newPid=(Adb @('shell','pidof',$fixturePackage)).Trim()
    if(!$newPid -or $newPid -eq $oldPid){throw 'Distinct restored process not proved'}
    $restored=Adb @('shell','dumpsys','activity','activities')
    if($restored -notmatch "topResumedActivity=[^\r\n]*dev.acportal.acceptance/[^\r\n]* t$taskId\}"){throw 'Previous task not restored'}
    $null=WaitNode 'Owned real draft 1'
    $cached=ReadUi
    $enabledCached=@($cached.SelectNodes('//node') | Where-Object {$_.GetAttribute('text') -in $decisionLabels -and $_.GetAttribute('enabled') -eq 'true'})
    if($enabledCached.Count){throw 'Cached decisions became enabled before host replay'}
    Write-Output 'CHECK cached draft restored before network; no enabled cached decisions'
    $null=Adb @('reverse',"tcp:$HostPort","tcp:$HostPort")
    $null=$ownedReverse.Add($HostPort)
    $reconnect=@((ReadUi).SelectNodes('//node') | Where-Object {$_.GetAttribute('text') -in @('Reconnect','Connect to host')}) | Select-Object -First 1
    if($reconnect){Tap ($reconnect.GetAttribute('text'))}
    $null=WaitNode ($permission.GetAttribute('text'))
    $authoritative=ReadUi
    $enabled=@($authoritative.SelectNodes('//node') | Where-Object {$_.GetAttribute('text') -in $decisionLabels -and $_.GetAttribute('enabled') -eq 'true'})
    if(!$enabled.Count){throw 'Authoritative real decisions remained unavailable'}
    Write-Output 'CHECK authoritative approval re-enabled only after replay'
    # Back dismisses the sheet, sending no decision, so the retained draft becomes visible.
    $null=Adb @('shell','input','keyevent','KEYCODE_BACK')
    $null=WaitNode 'Owned real draft 1'
    Tap '' 'Back'
    Tap 'Real recovery 2'
    $null=WaitNode 'Owned real draft 2'
    Tap '' 'Back'
    $null=WaitNode 'Real recovery 2'
    $null=Adb @('shell','input','keyevent','KEYCODE_HOME')
    Start-Sleep -Seconds 1
    $null=Adb @('shell','am','kill',$fixturePackage)
    Write-Output 'PASS real Goose Android: actual pending approval screen, distinct stopped/restored process, same task, retained draft and second live-session navigation; no decisions or prompts submitted'
} finally {
    $null=Adb @('shell','rm','-f',$fixtureXml)
    foreach($port in @($ownedReverse)) {$null=Adb @('reverse','--remove',"tcp:$port")}
}
