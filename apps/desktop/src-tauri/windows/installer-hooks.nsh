!define CONVENIENT_WINDOW_SHUTDOWN_EVENT "Local\com.ximizhou.convenientwindow.shutdown"
!define CONVENIENT_WINDOW_EVENT_MODIFY_STATE 0x0002

!macro NSIS_HOOK_PREUNINSTALL
  System::Call 'kernel32::OpenEventW(i ${CONVENIENT_WINDOW_EVENT_MODIFY_STATE}, i 0, w "${CONVENIENT_WINDOW_SHUTDOWN_EVENT}") p.r0'
  ${If} $0 P<> 0
    DetailPrint "Stopping Convenient Window and its helper..."
    System::Call 'kernel32::SetEvent(p r0) i.r1'
    System::Call 'kernel32::CloseHandle(p r0)'

    ${If} $1 <> 0
      StrCpy $R8 0
      convenient_window_wait_for_exit:
        nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
        Pop $R9
        ${If} $R9 <> 0
          Goto convenient_window_exit_done
        ${EndIf}
        Sleep 100
        IntOp $R8 $R8 + 1
        ${If} $R8 < 60
          Goto convenient_window_wait_for_exit
        ${EndIf}
      convenient_window_exit_done:
    ${EndIf}
  ${EndIf}
!macroend


!define CW_LEGACY_NAME "便捷窗口"
!define CW_LEGACY_KEY "Software\Microsoft\Windows\CurrentVersion\Uninstall\${CW_LEGACY_NAME}"
Var CwLegacyDirectory

; Upgrade the existing installation in place. Its path may be user-selected.
; Keep the identifier-based application data directory and WebView profile intact.
!macro NSIS_HOOK_PREINSTALL
  !insertmacro NSIS_HOOK_PREUNINSTALL
  StrCpy $CwLegacyDirectory ""
  ReadRegStr $R0 HKCU "${CW_LEGACY_KEY}" "Publisher"
  ReadRegStr $R1 HKCU "${CW_LEGACY_KEY}" "MainBinaryName"
  ${If} $R0 == "${MANUFACTURER}"
  ${AndIf} $R1 == "${MAINBINARYNAME}.exe"
    ReadRegStr $R2 HKCU "Software\${MANUFACTURER}\${CW_LEGACY_NAME}" ""
    ${If} $R2 != ""
    ${AndIf} ${FileExists} "$R2\${MAINBINARYNAME}.exe"
      ReadRegStr $R3 HKCU "${MANUPRODUCTKEY}" ""
      ${If} $R3 != ""
      ${AndIf} $R3 != $R2
      ${AndIf} ${FileExists} "$R3\${MAINBINARYNAME}.exe"
        MessageBox MB_OK|MB_ICONSTOP "Two installations were found. Uninstall the older Chinese-name installation before upgrading." /SD IDOK
        Abort
      ${EndIf}
      ReadRegStr $R3 HKCU "${CW_LEGACY_KEY}" "DisplayVersion"
      nsis_tauri_utils::SemverCompare "${VERSION}" $R3
      Pop $R4
      ${If} $R4 == -1
        MessageBox MB_OK|MB_ICONSTOP "A newer version is already installed." /SD IDOK
        Abort
      ${EndIf}
      StrCpy $CwLegacyDirectory $R2
      StrCpy $INSTDIR $R2
      SetOutPath $INSTDIR
    ${EndIf}
  ${EndIf}
!macroend

!macro CW_REMOVE_LEGACY_SHORTCUT PATH
  !insertmacro IsShortcutTarget "${PATH}" "$INSTDIR\${MAINBINARYNAME}.exe"
  Pop $R0
  ${If} $R0 == 1
    !insertmacro UnpinShortcut "${PATH}"
    Delete "${PATH}"
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTINSTALL
  ${If} $CwLegacyDirectory != ""
    ; Remove the old identity only after the new files and uninstall entry exist.
    DeleteRegKey HKCU "${CW_LEGACY_KEY}"
    DeleteRegKey HKCU "Software\${MANUFACTURER}\${CW_LEGACY_NAME}"
    !insertmacro CW_REMOVE_LEGACY_SHORTCUT "$DESKTOP\${CW_LEGACY_NAME}.lnk"
    !insertmacro CW_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\${CW_LEGACY_NAME}.lnk"
    !insertmacro CW_REMOVE_LEGACY_SHORTCUT "$SMPROGRAMS\${CW_LEGACY_NAME}\${CW_LEGACY_NAME}.lnk"
    RMDir "$SMPROGRAMS\${CW_LEGACY_NAME}"
  ${EndIf}
!macroend

!macro CW_REMOVE_STARTUP_ENTRY NAME
    ReadRegStr $R0 HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${NAME}"
    ${If} $R0 == '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" --autostart'
    ${OrIf} $R0 == '$\"$INSTDIR\${MAINBINARYNAME}.exe$\" --autostart --request-admin'
    ${OrIf} $R0 == '$INSTDIR\${MAINBINARYNAME}.exe --autostart'
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "${NAME}"
      DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${NAME}"
    ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ${If} $UpdateMode <> 1
    ; A silent upgrade may be uninstalled before the first application launch.
    !insertmacro CW_REMOVE_STARTUP_ENTRY "${CW_LEGACY_NAME}"
    !insertmacro CW_REMOVE_STARTUP_ENTRY "convenient-window"
    !insertmacro CW_REMOVE_STARTUP_ENTRY "${PRODUCTNAME}"
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run" "${PRODUCTNAME}"
  ${EndIf}
!macroend
