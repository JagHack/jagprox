; JagProx Windows installer. A per-user NSIS installer for the native launcher.
; cargo-packager's NSIS can't run post-install / pre-uninstall hooks, which JagProx
; needs for PATH management and the JagProx 2 migration, so the installer is written
; by hand here. The PATH work itself is done by the already-tested Rust command
; `jagprox.exe __windows-path add|remove`, so this script stays thin.
;
; Built by .github/workflows/release-rust.yml:
;   makensis -DVERSION=x.y.z -DSOURCEDIR=dir -DOUTFILE=setup.exe \
;            -DICON=icon.ico -DHEADER=header.bmp -DSIDEBAR=sidebar.bmp installer.nsi

Unicode true
ManifestDPIAware true

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "FileFunc.nsh"

!define APPNAME "JagProx"
!define MAINBIN "jagprox-launcher.exe"
; electron-builder's uninstall key for JagProx 2.x (UUID v5 of com.jaghack.jagprox).
!define JAGPROX2_GUID "59aab638-ecaa-5aab-9b5b-9a5e6fed9139"
!define UNINSTKEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${APPNAME}"

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef OUTFILE
  !define OUTFILE "JagProx-Setup.exe"
!endif
!ifndef SOURCEDIR
  !define SOURCEDIR "."
!endif

Name "${APPNAME}"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\JagProx"
RequestExecutionLevel user

!ifdef ICON
  !define MUI_ICON "${ICON}"
  !define MUI_UNICON "${ICON}"
!endif
!ifdef HEADER
  !define MUI_HEADERIMAGE
  !define MUI_HEADERIMAGE_BITMAP "${HEADER}"
!endif
!ifdef SIDEBAR
  !define MUI_WELCOMEFINISHPAGE_BITMAP "${SIDEBAR}"
  !define MUI_UNWELCOMEFINISHPAGE_BITMAP "${SIDEBAR}"
!endif
!define MUI_FINISHPAGE_RUN "$INSTDIR\${MAINBIN}"
!define MUI_FINISHPAGE_RUN_TEXT "Launch JagProx"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES
!insertmacro MUI_LANGUAGE "English"

; --- Remove JagProx 2.x (the Electron app) before installing. ---
!macro RemoveJagProx2
  ; Installed for the current user, in its own folder.
  ReadRegStr $R0 HKCU "Software\${JAGPROX2_GUID}" "InstallLocation"
  ${If} $R0 != ""
  ${AndIf} ${FileExists} "$R0\Uninstall JagProx.exe"
    DetailPrint "Removing JagProx 2..."
    ; _?= makes the uninstaller run in place, so ExecWait really waits for it.
    ExecWait '"$R0\Uninstall JagProx.exe" /currentuser /S _?=$R0'
    Delete "$R0\Uninstall JagProx.exe"
    RMDir "$R0"
  ${EndIf}
  ; Installed for all users (Program Files): removing it needs admin rights, so Windows asks.
  ReadRegStr $R0 HKLM "Software\${JAGPROX2_GUID}" "InstallLocation"
  ${If} $R0 != ""
  ${AndIf} ${FileExists} "$R0\Uninstall JagProx.exe"
    DetailPrint "Removing JagProx 2 (installed for all users)..."
    ExecShellWait "runas" "$R0\Uninstall JagProx.exe" "/allusers /S" SW_HIDE
  ${EndIf}
!macroend

Section "Install"
  !insertmacro RemoveJagProx2

  SetOutPath "$INSTDIR"
  ; 3.0.0-alpha downloaded updates into the default install folder; clear that out
  ; (only there: under a custom /D= an "updates" folder isn't ours).
  ${If} $INSTDIR == "$LOCALAPPDATA\JagProx"
    RMDir /r "$INSTDIR\updates"
  ${EndIf}

  ; A running launcher or jagprox (including a `jagprox update` that started us and
  ; is exiting) locks its .exe, and the silent update would fail to replace it.
  nsExec::Exec 'taskkill /IM ${MAINBIN} /F'
  Pop $0
  nsExec::Exec 'taskkill /IM jagprox.exe /F'
  Pop $0
  Sleep 500

  File "${SOURCEDIR}\${MAINBIN}"
  File "${SOURCEDIR}\jagprox.exe"
  File "${SOURCEDIR}\LICENSE"
  File "${SOURCEDIR}\NOTICE"

  WriteUninstaller "$INSTDIR\uninstall.exe"

  ; Add/Remove Programs entry.
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayName" "JagProx"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINSTKEY}" "Publisher" "JagHack"
  WriteRegStr HKCU "${UNINSTKEY}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINSTKEY}" "DisplayIcon" "$INSTDIR\${MAINBIN}"
  WriteRegStr HKCU "${UNINSTKEY}" "UninstallString" '"$INSTDIR\uninstall.exe"'
  WriteRegStr HKCU "${UNINSTKEY}" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINSTKEY}" "NoRepair" 1

  CreateShortcut "$SMPROGRAMS\JagProx.lnk" "$INSTDIR\${MAINBIN}"

  ; Put jagprox.exe on the user's PATH (REG_EXPAND_SZ, de-duplicated — in Rust).
  nsExec::Exec '"$INSTDIR\jagprox.exe" __windows-path add "$INSTDIR"'
  Pop $0

  ; Started by an updater (JagProx 2's and ours pass --force-run): start the new launcher.
  ${GetParameters} $1
  ClearErrors
  ${GetOptions} $1 "--force-run" $0
  ${IfNot} ${Errors}
  ${AndIf} ${Silent}
    Exec '"$INSTDIR\${MAINBIN}"'
  ${EndIf}
SectionEnd

Section "Uninstall"
  nsExec::Exec '"$INSTDIR\jagprox.exe" __windows-path remove "$INSTDIR"'
  Pop $0

  Delete "$INSTDIR\${MAINBIN}"
  Delete "$INSTDIR\jagprox.exe"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\NOTICE"
  Delete "$INSTDIR\uninstall.exe"
  ${If} $INSTDIR == "$LOCALAPPDATA\JagProx"
    RMDir /r "$INSTDIR\updates"
  ${EndIf}
  RMDir "$INSTDIR"

  Delete "$SMPROGRAMS\JagProx.lnk"
  DeleteRegKey HKCU "${UNINSTKEY}"
SectionEnd
