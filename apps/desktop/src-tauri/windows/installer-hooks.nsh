!define CONVENIENT_WINDOW_SHUTDOWN_EVENT "Local\com.ximizhou.convenientwindow.shutdown"
!define CONVENIENT_WINDOW_EVENT_MODIFY_STATE 0x0002

!macro NSIS_HOOK_PREUNINSTALL
  System::Call 'kernel32::OpenEventW(i ${CONVENIENT_WINDOW_EVENT_MODIFY_STATE}, i 0, w "${CONVENIENT_WINDOW_SHUTDOWN_EVENT}") p.r0'
  ${If} $0 P<> 0
    DetailPrint "正在关闭便捷窗口与后台助手..."
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
