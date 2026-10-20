!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
    DetailPrint "Waiting for KytyPS5 Studio to close before updating..."
    StrCpy $R8 0
    kyty_update_wait_for_executable:
      ; An exclusive open succeeds only after the running app releases its image.
      System::Call 'kernel32::CreateFileW(w "$INSTDIR\${MAINBINARYNAME}.exe", i 0x80000000, i 0, p 0, i 3, i 0x80, p 0) p.r9'
      ${If} $9 != -1
        System::Call 'kernel32::CloseHandle(p r9)'
        Goto kyty_update_executable_released
      ${EndIf}
      IntOp $R8 $R8 + 1
      ${If} $R8 < 80
        Sleep 250
        Goto kyty_update_wait_for_executable
      ${EndIf}
    IfSilent kyty_update_abort
    MessageBox MB_ICONSTOP|MB_OK "KytyPS5 Studio is still running. Close it completely and start the update again."
    kyty_update_abort:
      Abort "KytyPS5 Studio did not close before the update timed out."
    kyty_update_executable_released:
  ${EndIf}
!macroend
