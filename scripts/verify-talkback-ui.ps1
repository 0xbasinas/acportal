param(
    [string]$Serial='emulator-5554',
    [string]$Adb='C:/Users/basin/AppData/Local/Android/Sdk/platform-tools/adb.exe',
    [string]$TestClass='dev.acportal.presentation.TalkBackShellUiTest'
)
$ErrorActionPreference='Stop'
if($Serial -notmatch '^emulator-(\d+)$') {throw 'This fixture runner requires an Android emulator.'}
$consolePort=[int]$Matches[1]
if($TestClass -notmatch '^dev\.acportal\.presentation\.TalkBackShellUiTest(?:#[A-Za-z]+)?$') {throw 'Only the owned TalkBack fixture is supported.'}
if((& $Adb -s $Serial get-state).Trim() -ne 'device') {throw 'Emulator must be online.'}
$size=(& $Adb -s $Serial shell wm size) -join "`n"
if($size -notmatch 'Physical size: (\d+)x(\d+)' -or $size -match 'Override size:') {throw 'Verify native emulator dimensions before this fixture.'}
$screenWidth=[int]$Matches[1];$screenHeight=[int]$Matches[2]
$avd=(& $Adb -s $Serial emu avd name | Where-Object {$_.Trim() -ne 'OK'} | ForEach-Object {$_.Trim()}) -join ''
$api=(& $Adb -s $Serial shell getprop ro.build.version.sdk).Trim()
$density=(& $Adb -s $Serial shell wm density) -join ' '
$font=(& $Adb -s $Serial shell settings get system font_scale).Trim()
$rotation=(& $Adb -s $Serial shell settings get system accelerometer_rotation).Trim()
$userRotation=(& $Adb -s $Serial shell settings get system user_rotation).Trim()
Write-Output "Fixture emulator: $Serial, AVD=$avd, API=$api, ${screenWidth}x${screenHeight}, $density, font=$font, rotation=$rotation/$userRotation"
$baseline=@{}
foreach($key in @('enabled_accessibility_services','accessibility_enabled','touch_exploration_enabled')) {
    $baseline[$key]=(& $Adb -s $Serial shell settings get secure $key).Trim()
}
$client=[System.Net.Sockets.TcpClient]::new('127.0.0.1',$consolePort)
$stream=$client.GetStream();$stream.ReadTimeout=5000
$reader=[System.IO.StreamReader]::new($stream)
$writer=[System.IO.StreamWriter]::new($stream);$writer.AutoFlush=$true
function Read-ConsoleReply {
    while($true) {
        $line=$reader.ReadLine()
        if($null -eq $line) {throw 'Emulator console disconnected.'}
        $line=$line.Trim()
        if($line -eq 'OK') {return}
        if($line.StartsWith('KO:')) {throw "Emulator console rejected fixture input: $line"}
    }
}
function Send-ConsoleCommand([string]$Command) {
    $writer.WriteLine($Command)
    Read-ConsoleReply
}
$run=$null
try {
    Read-ConsoleReply
    $consoleToken=(Get-Content -LiteralPath (Join-Path $env:USERPROFILE '.emulator_console_auth_token') -Raw).Trim()
    Send-ConsoleCommand ('auth '+$consoleToken)
    $consoleToken=$null
    $start=[System.Diagnostics.ProcessStartInfo]::new($Adb)
    $start.UseShellExecute=$false;$start.CreateNoWindow=$true;$start.RedirectStandardOutput=$true
    foreach($argument in @('-s',$Serial,'shell','am','instrument','-w','-r','-e','talkback','true','-e','hardwareGestures','true','-e','class',$TestClass,'dev.acportal.test/androidx.test.runner.AndroidJUnitRunner')) {$start.ArgumentList.Add($argument)}
    $run=[System.Diagnostics.Process]::Start($start)
    $passed=$false;$failed=$false
    while($null -ne ($line=$run.StandardOutput.ReadLine())) {
        Write-Output $line
        if($line -eq 'INSTRUMENTATION_STATUS: acportalGesture=swipeRight') {
            $top=(& $Adb -s $Serial shell dumpsys activity activities | Select-String 'topResumedActivity') -join ' '
            if($top -notmatch 'dev\.acportal/\.presentation\.UiAccessibilityFixtureActivity') {throw 'Hardware input requires the owned fixture in the foreground.'}
            $y=[int]($screenHeight*0.5)
            try {
                for($step=0;$step -le 8;$step++) {
                    $x=[int]($screenWidth*(0.25+$step*0.0625))
                    $buttons=if($step -eq 8) {0} else {1}
                    Send-ConsoleCommand "event mouse $x $y 0 $buttons"
                    Start-Sleep -Milliseconds 15
                }
            } finally {Send-ConsoleCommand "event mouse $x $y 0 0"}
        }
        if($line -match '^OK \(\d+ tests?\)') {$passed=$true}
        if($line -match '^FAILURES!!!|INSTRUMENTATION_FAILED|Process crashed') {$failed=$true}
    }
    $run.WaitForExit()
    if($run.ExitCode -ne 0 -or -not $passed -or $failed) {throw 'TalkBack fixture did not pass. Inspect terminal output.'}
} finally {
    # Let a started instrumentation finish and execute its own settings restoration.
    if($null -ne $run) {
        while($null -ne ($remaining=$run.StandardOutput.ReadLine())) {Write-Output $remaining}
        $run.WaitForExit();$run.Dispose()
    }
    $writer.Dispose();$reader.Dispose();$client.Dispose()
    foreach($key in $baseline.Keys) {
        $current=(& $Adb -s $Serial shell settings get secure $key).Trim()
        if($current -ne $baseline[$key]) {throw "Fixture failed to restore secure setting $key."}
    }
}
