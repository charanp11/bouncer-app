; Bouncer's steps in Tauri's NSIS uninstaller.

; Before the files go: offer to take Bouncer's hooks out of Claude Code's
; settings. It is `bouncer uninstall-hooks`: the diff, yes / no, a dated
; backup and an atomic write; nothing without a "y". Skipped in an update and
; in silent or passive runs, which leave the hooks as they are.
!macro NSIS_HOOK_PREUNINSTALL
  ${If} $UpdateMode <> 1
  ${AndIf} $PassiveMode <> 1
  ${AndIfNot} ${Silent}
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
