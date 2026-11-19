; JagProx additions to the Tauri NSIS installer (bundle.windows.nsis.installerHooks).

; electron-builder's uninstall key for JagProx 2.x (UUID v5 of com.jaghack.jagprox).
!define JAGPROX2_GUID "59aab638-ecaa-5aab-9b5b-9a5e6fed9139"

!macro NSIS_HOOK_PREINSTALL
  ; JagProx 2.x was the Electron app in its own folder. Remove it (its settings in
  ; %APPDATA%\jagprox-launcher stay, and JagProx 3 uses them).
  ReadRegStr $R0 HKCU "Software\${JAGPROX2_GUID}" "InstallLocation"
  ${If} $R0 != ""
  ${AndIf} ${FileExists} "$R0\Uninstall JagProx.exe"
    DetailPrint "Removing JagProx 2..."
    ; _?= makes the uninstaller run in place, so ExecWait really waits for it.
    ExecWait '"$R0\Uninstall JagProx.exe" /currentuser /S _?=$R0'
    Delete "$R0\Uninstall JagProx.exe"
    RMDir "$R0"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ; Put jagprox.exe on the user's PATH.
  nsExec::Exec '"$INSTDIR\jagprox.exe" __windows-path add "$INSTDIR"'
  Pop $R0

  ; Started by JagProx 2's updater (it passes --force-run): start the new launcher.
  ${GetOptions} $CMDLINE "--force-run" $R0
  ${IfNot} ${Errors}
  ${AndIf} ${Silent}
    Exec '"$INSTDIR\${MAINBINARYNAME}.exe"'
  ${EndIf}
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  nsExec::Exec '"$INSTDIR\jagprox.exe" __windows-path remove "$INSTDIR"'
  Pop $R0
!macroend
