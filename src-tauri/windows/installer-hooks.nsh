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

!macro KVIEW_REGISTER_CAPABILITY EXT
  WriteRegStr SHELL_CONTEXT "Software\KView\Capabilities\FileAssociations" ".${EXT}" "KView 支持的文件"
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
  !insertmacro ${ACTION} "config"
  !insertmacro ${ACTION} "properties"
  !insertmacro ${ACTION} "sql"
  !insertmacro ${ACTION} "csv"
  !insertmacro ${ACTION} "tsv"
  !insertmacro ${ACTION} "md"
  !insertmacro ${ACTION} "markdown"
  !insertmacro ${ACTION} "js"
  !insertmacro ${ACTION} "jsx"
  !insertmacro ${ACTION} "mjs"
  !insertmacro ${ACTION} "cjs"
  !insertmacro ${ACTION} "ts"
  !insertmacro ${ACTION} "tsx"
  !insertmacro ${ACTION} "mts"
  !insertmacro ${ACTION} "cts"
  !insertmacro ${ACTION} "vue"
  !insertmacro ${ACTION} "html"
  !insertmacro ${ACTION} "htm"
  !insertmacro ${ACTION} "css"
  !insertmacro ${ACTION} "scss"
  !insertmacro ${ACTION} "sass"
  !insertmacro ${ACTION} "less"
  !insertmacro ${ACTION} "py"
  !insertmacro ${ACTION} "java"
  !insertmacro ${ACTION} "kt"
  !insertmacro ${ACTION} "kts"
  !insertmacro ${ACTION} "c"
  !insertmacro ${ACTION} "h"
  !insertmacro ${ACTION} "cc"
  !insertmacro ${ACTION} "cpp"
  !insertmacro ${ACTION} "cxx"
  !insertmacro ${ACTION} "hpp"
  !insertmacro ${ACTION} "cs"
  !insertmacro ${ACTION} "go"
  !insertmacro ${ACTION} "rs"
  !insertmacro ${ACTION} "php"
  !insertmacro ${ACTION} "rb"
  !insertmacro ${ACTION} "swift"
  !insertmacro ${ACTION} "dart"
  !insertmacro ${ACTION} "lua"
  !insertmacro ${ACTION} "sh"
  !insertmacro ${ACTION} "bash"
  !insertmacro ${ACTION} "zsh"
  !insertmacro ${ACTION} "fish"
  !insertmacro ${ACTION} "ps1"
  !insertmacro ${ACTION} "bat"
  !insertmacro ${ACTION} "cmd"
  !insertmacro ${ACTION} "graphql"
  !insertmacro ${ACTION} "gql"
  !insertmacro ${ACTION} "proto"
  !insertmacro ${ACTION} "gitignore"
  !insertmacro ${ACTION} "gitattributes"
  !insertmacro ${ACTION} "editorconfig"
  !insertmacro ${ACTION} "pdf"
  !insertmacro ${ACTION} "doc"
  !insertmacro ${ACTION} "docx"
  !insertmacro ${ACTION} "xls"
  !insertmacro ${ACTION} "xlsx"
  !insertmacro ${ACTION} "zip"
  !insertmacro ${ACTION} "tar"
  !insertmacro ${ACTION} "tgz"
  !insertmacro ${ACTION} "gz"
  !insertmacro ${ACTION} "mp4"
  !insertmacro ${ACTION} "webm"
!macroend

!macro NSIS_HOOK_PREINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_PRESERVE_ASSOCIATION
!macroend

!macro NSIS_HOOK_POSTINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_RESTORE_ASSOCIATION_BACKUP
  WriteRegStr SHELL_CONTEXT "Software\KView\Capabilities" "ApplicationName" "KView"
  WriteRegStr SHELL_CONTEXT "Software\KView\Capabilities" "ApplicationDescription" "使用 KView 快速查看文件"
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_REGISTER_CAPABILITY
  WriteRegStr SHELL_CONTEXT "Software\RegisteredApplications" "KView" "Software\KView\Capabilities"
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro KVIEW_FOR_EACH_ASSOCIATION KVIEW_CLEAN_ASSOCIATION_BACKUP
  DeleteRegValue SHELL_CONTEXT "Software\RegisteredApplications" "KView"
  DeleteRegKey SHELL_CONTEXT "Software\KView"
  DeleteRegValue SHELL_CONTEXT "Software\Microsoft\Windows\CurrentVersion\Run" "KView"
!macroend
