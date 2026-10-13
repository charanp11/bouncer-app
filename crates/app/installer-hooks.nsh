; Bouncer's steps in Tauri's NSIS uninstaller.

; Before the files go: offer to take Bouncer's hooks out of Claude Code's
; settings. It is `bouncer uninstall-hooks`: the diff, yes / no (no is the
; default), a dated backup and an atomic write; nothing without a "y".
;
; Only when a person uninstalls. Never in an update (/UPDATE), a silent or
; passive run, or when an installer runs this uninstaller: an upgrade's
; "uninstall before installing" (Tauri's reinstall page) passes `_?=<install
; dir>`, which makes NSIS run the uninstaller in place, from the install
; folder. Started by a person (Settings > Apps), NSIS first copies it to a
; temp folder (~nsuN.tmp\Un.exe) and runs that copy. So: running beside
; bouncer-app.exe means an installer started it, and the hooks stay.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
  ${AndIfNot} ${FileExists} "$EXEDIR\bouncer-app.exe"
  ${AndIf} ${FileExists} "$INSTDIR\bouncer.exe"
    DetailPrint "Asking whether to remove Bouncer's hooks from Claude Code's settings"
    ExecWait '"$INSTDIR\bouncer.exe" uninstall-hooks --uninstalling'
  ${EndIf}
!macroend

; After: "Delete app data" (unticked by default) also removes Bouncer's own
; folder (rules, preferences, history, the chat folder). Tauri's own step
; removes the WebView2 folder ($LOCALAPPDATA\<identifier>). Nothing else.
!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    SetShellVarContext current
    RmDir /r "$APPDATA\Bouncer"
  ${EndIf}
!macroend
