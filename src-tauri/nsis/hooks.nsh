; ZANPOS custom NSIS hooks
; Runs before the uninstall section removes the install directory.
; Always wipes the WhatsApp session folder so no lingering auth tokens
; are left on disk after uninstall.  The main SQLite database and other
; app data survive unless the user also ticks "Delete app data".

!macro NSIS_HOOK_POSTINSTALL
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="ZANPOS Hub"'
  nsExec::ExecToLog 'netsh advfirewall firewall add rule name="ZANPOS Hub" dir=in action=allow program="$INSTDIR\ZANPOS.exe" protocol=TCP profile=private,domain'
!macroend

!macro NSIS_HOOK_PREUNINSTALL
  SetShellVarContext current
  RMDir /r "$APPDATA\com.super.zanpos\wa-session"
  nsExec::ExecToLog 'netsh advfirewall firewall delete rule name="ZANPOS Hub"'
!macroend
