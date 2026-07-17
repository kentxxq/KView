!macro KVIEW_PRESERVE_ASSOCIATION EXT
  ReadRegStr $R0 SHELL_CONTEXT "Software\Classes\.${EXT}" ""
  ${If} $R0 == "KView 支持的文件"
    ReadRegStr $R1 SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_backup"
    WriteRegStr SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_backup" "$R1"
    WriteRegStr SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_marker" "1"
  ${EndIf}
!macroend

!macro KVIEW_RESTORE_ASSOCIATION_BACKUP EXT
  ReadRegStr $R0 SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_marker"
  ${If} $R0 == "1"
    ReadRegStr $R1 SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_backup"
    WriteRegStr SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_backup" "$R1"
    DeleteRegValue SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_backup"
    DeleteRegValue SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_marker"
  ${EndIf}
!macroend

!macro KVIEW_CLEAN_ASSOCIATION_BACKUP EXT
  DeleteRegValue SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_backup"
  DeleteRegValue SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_backup"
  DeleteRegValue SHELL_CONTEXT "Software\Classes\.${EXT}" "KView 支持的文件_upgrade_marker"
!macroend

!macro KVIEW_FOR_EACH_ASSOCIATION ACTION
  !insertmacro ${ACTION} "jpg"
  !insertmacro ${ACTION} "jpeg"
  !insertmacro ${ACTION} "png"
  !insertmacro ${ACTION} "webp"
  !insertmacro ${ACTION} "gif"
  !insertmacro ${ACTION} "svg"
  !insertmacro ${ACTION} "txt"
  !insertmacro ${ACTION} "log"
  !insertmacro ${ACTION} "json"
  !insertmacro ${ACTION} "yaml"
  !insertmacro ${ACTION} "yml"
  !insertmacro ${ACTION} "xml"
  !insertmacro ${ACTION} "toml"
  !insertmacro ${ACTION} "ini"
  !insertmacro ${ACTION} "env"
  !insertmacro ${ACTION} "conf"
  !insertmacro ${ACTION} "sql"
  !insertmacro ${ACTION} "md"
  !insertmacro ${ACTION} "markdown"
  !insertmacro ${ACTION} "pdf"
  !insertmacro ${ACTION} "docx"
  !insertmacro ${ACTION} "xlsx"
  !insertmacro ${ACTION} "mp4"
  !insertmacro ${ACTION} "webm"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_PRESERVE_ASSOCIATION
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_RESTORE_ASSOCIATION_BACKUP
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_CLEAN_ASSOCIATION_BACKUP
  DeleteRegValue SHELL_CONTEXT "Software\Microsoft\Windows\CurrentVersion\Run" "KView"
!macroend
