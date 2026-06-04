; ZANPOS custom NSIS hooks
; Runs before the uninstall section removes the install directory.
; Always wipes the WhatsApp session folder so no lingering auth tokens
; are left on disk after uninstall.  The main SQLite database and other
; app data survive unless the user also ticks "Delete app data".

!macro NSIS_HOOK_PREUNINSTALL
  SetShellVarContext current
  ; Remove only the WhatsApp session — leaves the POS database intact.
  RMDir /r "$APPDATA\com.super.zanpos\wa-session"
!macroend
