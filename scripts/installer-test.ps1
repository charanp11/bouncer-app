# The installer, end to end, on a disposable Windows machine (CI's fresh
# windows-latest runner). Never run this on a real profile: it installs,
# uninstalls and deletes Bouncer's data folders.
#
#   installer-test.ps1 -Old <setup.exe> -OldVersion 0.0.1 -New <setup.exe> -NewVersion 0.0.2 -Fixtures <dir>
#
# Steps (TESTS W4-W7, PLAN "c-5 review fixes"): install, hooks with the
# installed bouncer.exe, a card through the installed hook, Start with Windows
# on / off from Settings, an upgrade through "uninstall before installing" (our
# hooks console must never open), uninstall with the data box unticked
# (settings.json back byte for byte), uninstall with it ticked (data gone).
# The windows are driven with UI Automation and button messages; the hooks
# console's answers are written into that console's input only.
param(
    [string]$Old, [string]$OldVersion, [string]$New, [string]$NewVersion, [string]$Fixtures,
    # Internal: attach to the console of process $ConsoleOf, save its text to
    # $Out, then type $Answer + Enter into it.
    [int]$ConsoleOf = 0, [string]$Out = "", [string]$Answer = ""
)
$ErrorActionPreference = "Stop"

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;
public static class Native {
  [DllImport("user32.dll")] public static extern IntPtr SendMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern bool PostMessageW(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetDlgCtrlID(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetParent(IntPtr h);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("kernel32.dll")] public static extern bool FreeConsole();
  [DllImport("kernel32.dll")] public static extern bool AttachConsole(uint pid);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
  static extern IntPtr CreateFileW(string n, uint a, uint s, IntPtr sa, uint c, uint f, IntPtr t);
  [StructLayout(LayoutKind.Explicit, Size = 20)]
  struct KeyRec {
    [FieldOffset(0)] public ushort EventType; [FieldOffset(4)] public int KeyDown;
    [FieldOffset(8)] public ushort Repeat; [FieldOffset(10)] public ushort Vk;
    [FieldOffset(12)] public ushort Scan; [FieldOffset(14)] public char Ch; [FieldOffset(16)] public uint Ctrl;
  }
  [DllImport("kernel32.dll")] static extern bool WriteConsoleInputW(IntPtr h, KeyRec[] r, uint n, out uint w);
  [StructLayout(LayoutKind.Sequential)] struct Coord { public short X, Y; }
  [StructLayout(LayoutKind.Sequential)] struct Info { public Coord Size, Cursor; public short L, T, R, B; public ushort Attr; public Coord Max; }
  [DllImport("kernel32.dll")] static extern bool GetConsoleScreenBufferInfo(IntPtr h, out Info i);
  [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
  static extern bool ReadConsoleOutputCharacterW(IntPtr h, StringBuilder b, uint n, Coord at, out uint read);
  public static string Screen() {
    IntPtr o = CreateFileW("CONOUT$", 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
    Info i; GetConsoleScreenBufferInfo(o, out i);
    var sb = new StringBuilder();
    for (short y = 0; y <= i.Cursor.Y; y++) {
      var line = new StringBuilder(i.Size.X); uint r;
      ReadConsoleOutputCharacterW(o, line, (uint)i.Size.X, new Coord { X = 0, Y = y }, out r);
      sb.AppendLine(line.ToString(0, (int)r).TrimEnd());
    }
    return sb.ToString();
  }
  public static void Type(string text) {
    IntPtr h = CreateFileW("CONIN$", 0xC0000000, 3, IntPtr.Zero, 3, 0, IntPtr.Zero);
    var recs = new System.Collections.Generic.List<KeyRec>();
    foreach (char c in text + "\r") {
      ushort vk = c == '\r' ? (ushort)0x0D : (ushort)char.ToUpper(c);
      recs.Add(new KeyRec { EventType = 1, KeyDown = 1, Repeat = 1, Vk = vk, Ch = c });
      recs.Add(new KeyRec { EventType = 1, KeyDown = 0, Repeat = 1, Vk = vk, Ch = c });
    }
    uint w; WriteConsoleInputW(h, recs.ToArray(), (uint)recs.Count, out w);
  }
}
'@

if ($ConsoleOf) {
    [Native]::FreeConsole() | Out-Null
    if (-not [Native]::AttachConsole($ConsoleOf)) { "attach failed" | Set-Content $Out; exit 1 }
    [Native]::Screen() | Set-Content $Out -Encoding utf8
    if ($PSBoundParameters.ContainsKey("Answer")) { [Native]::Type($Answer.Trim()) }  # " " = just Enter
    [Native]::FreeConsole() | Out-Null
    exit 0
}

Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$UIA = [System.Windows.Automation.AutomationElement]
$Scope = [System.Windows.Automation.TreeScope]
$True_ = [System.Windows.Automation.Condition]::TrueCondition

$dir = Join-Path $env:LOCALAPPDATA "Bouncer"
$data = Join-Path $env:APPDATA "Bouncer"
$webview = Join-Path $env:LOCALAPPDATA "io.github.charanp11.bouncer"
$uninstallKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Bouncer"
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Run"
$work = Join-Path $env:RUNNER_TEMP "installer-test"
$cfg = Join-Path $work "claude"
New-Item -ItemType Directory -Force $cfg | Out-Null
$settings = Join-Path $cfg "settings.json"
$original = "{`n  `"model`": `"opus`",`n  `"hooks`": {`n    `"Stop`": [`n      {`n        `"hooks`": [`n          {`n            `"type`": `"command`",`n            `"command`": `"notify-send done`"`n          }`n        ]`n      }`n    ]`n  }`n}`n"
[IO.File]::WriteAllText($settings, $original)
$originalHash = (Get-FileHash $settings).Hash
# Everything started from here (installers, uninstallers, the app) uses it.
$env:CLAUDE_CONFIG_DIR = $cfg

$results = [System.Collections.Generic.List[string]]::new()
function Pass($step, $detail) { $results.Add("| $step | pass | $detail |"); Write-Host "PASS  $step  $detail" }
function Check($ok, $step, $detail) {
    if (-not $ok) { $results.Add("| $step | **FAIL** | $detail |"); throw "FAILED: ${step}: $detail" }
    Pass $step $detail
}
function Summary {
    $table = @("| Step | | Result |", "| --- | --- | --- |") + $results
    if ($env:GITHUB_STEP_SUMMARY) { ($table -join "`n") | Add-Content $env:GITHUB_STEP_SUMMARY -Encoding utf8 }
    $table | Write-Host
}
function Wait-Until([scriptblock]$cond, [int]$seconds = 30) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) { $v = & $cond; if ($v) { return $v }; Start-Sleep -Milliseconds 250 }
    $null
}
function Windows-Named($pattern) {
    $UIA::RootElement.FindAll($Scope::Children, $True_) | Where-Object { $_.Current.Name -like $pattern }
}
# The first enabled element named $name (of $type, if given: the page's
# labels share names with their buttons, so filter before taking the first).
function Control($window, $name, $type = "") {
    $window.FindAll($Scope::Descendants, $True_) |
        Where-Object { $_.Current.Name -like $name -and $_.Current.IsEnabled -and (-not $type -or $_.Current.ControlType.ProgrammaticName -eq $type) } |
        Select-Object -First 1
}
function Button($window, $name) { Control $window $name "ControlType.Button" }
# Presses a web button. The island re-renders often, so an element can be gone
# by the time it's pressed: that's not an error, the caller looks again.
function Press($button) {
    try {
        # A switch (role="switch") only toggles; other buttons are invoked.
        $p = $null
        if ($button.TryGetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern, [ref]$p)) { $p.Invoke() }
        elseif ($button.TryGetCurrentPattern([System.Windows.Automation.TogglePattern]::Pattern, [ref]$p)) { $p.Toggle() }
        else { return $false }
        $true
    } catch [System.Windows.Automation.ElementNotAvailableException] { $false }
}
function Click($control) {
    # A button's click as its dialog hears it (no mouse, no keys).
    $h = [IntPtr]$control.Current.NativeWindowHandle
    [void][Native]::PostMessageW([Native]::GetParent($h), 0x0111, [IntPtr][Native]::GetDlgCtrlID($h), $h)
}
# What UI Automation sees in a window, for the log when a lookup fails.
function Dump($window, $why) {
    Write-Host "---- $why; the window's elements:"
    $window.FindAll($Scope::Descendants, $True_) | ForEach-Object {
        "  {0} | {1} | enabled={2}" -f $_.Current.ControlType.ProgrammaticName, $_.Current.Name, $_.Current.IsEnabled
    } | Select-Object -First 200 | Write-Host
    throw $why
}
function Checked($control) { [int][Native]::SendMessageW([IntPtr]$control.Current.NativeWindowHandle, 0xF0, 0, 0) -eq 1 }
function Set-Checked($control, $on) { [void][Native]::SendMessageW([IntPtr]$control.Current.NativeWindowHandle, 0xF1, [IntPtr][int]$on, 0) }
function Console-Answer($procId, $answer, $file) {
    & powershell -NoProfile -ExecutionPolicy Bypass -File $PSCommandPath -ConsoleOf $procId -Out $file -Answer " $answer"
    Get-Content $file -Raw
}
function Hooks-Console { Get-CimInstance Win32_Process -Filter "Name='bouncer.exe'" | Select-Object -First 1 }
function Installed-Version { (Get-ItemProperty $uninstallKey -ErrorAction SilentlyContinue).DisplayVersion }
function Start-App {
    $p = Start-Process (Join-Path $dir "bouncer-app.exe") -PassThru
    $sid = [System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value
    $pipe = Wait-Until { [System.IO.Directory]::GetFiles("\\.\pipe\") -contains "\\.\pipe\bouncer-$sid" } 30
    if (-not $pipe) { throw "the installed app never opened its pipe" }
    $p
}
function Stop-App($p) { if ($p -and -not $p.HasExited) { Stop-Process -Id $p.Id -Force; $p.WaitForExit(10000) | Out-Null } }

try {
    # 1. Install.
    $p = Start-Process $Old -ArgumentList "/S" -PassThru -Wait
    $files = (Get-ChildItem $dir -ErrorAction SilentlyContinue).Name -join ", "
    Check ($p.ExitCode -eq 0 -and $files -eq "bouncer-app.exe, bouncer-hook.exe, bouncer.exe, uninstall.exe") "Install $OldVersion (/S)" "exit $($p.ExitCode); $files"
    Check ((Installed-Version) -eq $OldVersion -and -not (Test-Path "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Bouncer")) "Per user" "HKCU uninstall entry $(Installed-Version), none in HKLM; $dir"

    # 2. Hooks with the installed bouncer.exe.
    cmd /c "echo y| `"$dir\bouncer.exe`" install-hooks" | Out-Null
    $hook = ((Get-Content $settings -Raw | ConvertFrom-Json).hooks.PermissionRequest | ForEach-Object { $_.hooks } | Where-Object { $_.command -like "*bouncer-hook.exe" }).command
    Check ($hook -eq (Join-Path $dir "bouncer-hook.exe")) "Hooks via bouncer.exe" "command $hook"
    $hooked = (Get-FileHash $settings).Hash

    # 3. A card through the installed hook.
    $app = Start-App
    $relay = Start-Process cmd -ArgumentList "/c `"`"$hook`" < `"$Fixtures\claude-code-2.1.143-PermissionRequest-Bash.json`"`"" -PassThru -WindowStyle Hidden
    Start-Sleep 10
    $waiting = Get-CimInstance Win32_Process -Filter "ParentProcessId=$($relay.Id)" | Where-Object Name -eq "bouncer-hook.exe"
    Check ([bool]$waiting) "A card" "a permission request through $hook still waiting after 10 s"
    Stop-Process -Id $waiting.ProcessId -Force

    # 4. Start with Windows, on and off, from Settings (Ctrl+Alt+B opens the island).
    Start-Sleep 2
    foreach ($vk in 0x11, 0x12, 0x42) { [Native]::keybd_event($vk, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40 }
    foreach ($vk in 0x42, 0x12, 0x11) { [Native]::keybd_event($vk, 0, 2, [UIntPtr]::Zero); Start-Sleep -Milliseconds 40 }
    $island = Wait-Until { Windows-Named "Bouncer" | Where-Object { $_.Current.ProcessId -eq $app.Id } | Select-Object -First 1 } 15
    if (-not $island) { throw "no Bouncer window" }
    # A runner sits idle, so the key press counts as coming back: "While you
    # were away" covers the island until closed. Close it whenever it shows.
    $awayGone = {
        if (Control $island "While you were away") {
            $c = Button $island "Close"
            if ($c) { [void](Press $c) }
        }
    }
    # Closing the summary folds the island back to its pill ("1 session · …"):
    # press the pill to open it again.
    $opened = {
        & $awayGone
        $pill = Button $island "*session*"
        if ($pill -and -not (Button $island "Settings")) { [void](Press $pill) }
    }
    $gear = Wait-Until { & $opened; Button $island "Settings" } 30
    if (-not $gear) { Dump $island "no Settings button in the island" }
    [void](Press $gear)
    $switch = Wait-Until {
        & $opened
        if (-not (Control $island "Rules file")) {  # not in Settings (the summary took over): the gear again
            $g = Button $island "Settings"
            if ($g) { [void](Press $g) }
        }
        Button $island "Start with Windows"
    } 30
    if (-not $switch) { Dump $island "no Start with Windows switch in Settings" }
    # Press the switch until the Run value shows the state wanted (at most one
    # press per 2 s, so a slow write is never flipped back).
    $setRun = {
        param($on)
        $is = { $v = (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).Bouncer; if ($on) { $v -eq $want } else { -not $v } }
        $end = (Get-Date).AddSeconds(30)
        while (-not (& $is) -and (Get-Date) -lt $end) {
            & $opened
            $s = Button $island "Start with Windows"
            if ($s) { [void](Press $s) }
            $wait = (Get-Date).AddSeconds(2)
            while (-not (& $is) -and (Get-Date) -lt $wait) { Start-Sleep -Milliseconds 200 }
        }
        & $is
    }
    $want = "`"$(Join-Path $dir 'bouncer-app.exe')`""
    Check (-not (Get-ItemProperty $runKey -ErrorAction SilentlyContinue).Bouncer) "Start with Windows: off by default" "no Run value"
    Check (& $setRun $true) "Start with Windows: on" "Run value Bouncer = $want"
    Check (& $setRun $false) "Start with Windows: off" "Run value removed"
    Stop-App $app

    # 5. Upgrade through "uninstall before installing": our console must never open.
    $seen = Join-Path $work "hooks-console-seen.txt"
    $watch = Start-Job -ArgumentList $seen -ScriptBlock {
        param($seen)
        while ($true) {
            if (Get-Process bouncer -ErrorAction SilentlyContinue) { "seen" | Set-Content $seen }
            Start-Sleep -Milliseconds 100
        }
    }
    $setup = Start-Process $New -PassThru
    $sawReinstallPage = $false
    $end = (Get-Date).AddMinutes(4)
    while (-not $setup.HasExited -and (Get-Date) -lt $end) {
        foreach ($w in @(Windows-Named "*Bouncer*")) {
            $radio = Control $w "*ninstall before installing*"
            if ($radio) { $sawReinstallPage = $sawReinstallPage -or (Checked $radio) }
            $run = Control $w "Run Bouncer*"
            if ($run) { Set-Checked $run 0 }
            foreach ($name in "Uninstall", "Next >", "Install", "Close", "Finish") {
                $b = Control $w $name
                if ($b) { Click $b; Start-Sleep -Milliseconds 800; break }
            }
        }
        Start-Sleep -Milliseconds 500
    }
    Stop-Job $watch; Remove-Job $watch
    Check ($setup.HasExited) "Upgrade to $NewVersion" "the installer finished (exit $($setup.ExitCode))"
    Check $sawReinstallPage "Upgrade: the reinstall page" "'Uninstall before installing' was offered and ticked"
    Check (-not (Test-Path $seen)) "Upgrade: no hooks console" "bouncer.exe never ran while the old uninstaller ran"
    Check ((Installed-Version) -eq $NewVersion -and (Get-FileHash $settings).Hash -eq $hooked -and (Test-Path $hook)) "Upgrade: hooks kept" "version $(Installed-Version); settings.json unchanged; $hook exists"
    $publisher = (Get-ItemProperty $uninstallKey -ErrorAction SilentlyContinue).Publisher
    Check ($publisher -eq "Charan") "Publisher" "Installed apps shows '$publisher'"

    # 6. Uninstall, data box unticked: hooks removed through the console.
    $app = Start-App; Start-Sleep 5; Stop-App $app   # so there is data to keep
    Start-Process (Join-Path $dir "uninstall.exe")
    $w = Wait-Until { Windows-Named "Bouncer Uninstall*" | Select-Object -First 1 } 30
    $box = Wait-Until { Control $w "Delete the application data" } 15
    Check (-not (Checked $box)) "Uninstall: data box" "unticked by default"
    Click (Control $w "Uninstall")
    $con = Wait-Until { Hooks-Console } 30
    if (-not $con) { throw "the hooks console never opened" }
    Start-Sleep 1
    $text = Console-Answer $con.ProcessId "y" (Join-Path $work "console-1.txt")
    Check ($text -match "Answer N to keep them" -and $text -match "\[y/N\]") "Uninstall: hooks console" "diff and the why above [y/N]; answered y"
    Start-Sleep 2
    [void](Console-Answer $con.ProcessId "" (Join-Path $work "console-2.txt"))
    $close = Wait-Until { Control $w "Close" } 60
    Click $close
    Wait-Until { -not (Test-Path (Join-Path $dir "bouncer-app.exe")) } 30 | Out-Null
    $backups = (Get-ChildItem $cfg -Filter "settings.json.bouncer-backup-*").Count
    Check ((Get-FileHash $settings).Hash -eq $originalHash) "Uninstall: settings.json" "back byte for byte ($backups backups)"
    Check (-not (Test-Path (Join-Path $dir "bouncer-app.exe")) -and -not (Installed-Version)) "Uninstall: program" "files and uninstall entry gone"
    Check ((Test-Path $data) -and (Test-Path $webview)) "Uninstall unticked: data kept" "$data and $webview still there"

    # 7. Reinstall, then uninstall with the box ticked: the data goes.
    $p = Start-Process $New -ArgumentList "/S" -PassThru -Wait
    $app = Start-App; Start-Sleep 5; Stop-App $app
    Start-Process (Join-Path $dir "uninstall.exe")
    $w = Wait-Until { Windows-Named "Bouncer Uninstall*" | Select-Object -First 1 } 30
    $box = Wait-Until { Control $w "Delete the application data" } 15
    Set-Checked $box 1
    Click (Control $w "Uninstall")
    $con = Wait-Until { Hooks-Console } 30
    if ($con) { Start-Sleep 1; [void](Console-Answer $con.ProcessId "" (Join-Path $work "console-3.txt")) }  # no hooks left: "Nothing to change."
    $close = Wait-Until { Control $w "Close" } 60
    Click $close
    Wait-Until { -not (Test-Path $data) } 30 | Out-Null
    Check (-not (Test-Path $data) -and -not (Test-Path $webview)) "Uninstall ticked: data gone" "$data and $webview removed"
    Check ((Get-FileHash $settings).Hash -eq $originalHash) "End: settings.json" "the original, byte for byte"
} finally {
    if ($results.Count) { Summary }
}
