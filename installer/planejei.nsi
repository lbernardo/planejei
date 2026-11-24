; Instalador do Planejei para Windows (NSIS 3).
;
; Gerado pelo scripts/build.sh:
;   makensis -DVERSION=0.1.0 -DSRC=dist/windows -DOUT=dist/Planejei-0.1.0-setup.exe installer/planejei.nsi
;
; Instala só para o usuário atual (%LOCALAPPDATA%\Programs\Planejei): não pede administrador.

Unicode true
ManifestDPIAware true
RequestExecutionLevel user
SetCompressor /SOLID lzma

!ifndef VERSION
  !define VERSION "0.0.0"
!endif
!ifndef SRC
  !define SRC "..\dist\windows"
!endif
!ifndef OUT
  !define OUT "Planejei-${VERSION}-setup.exe"
!endif

!define APP      "Planejei"
!define EXE      "Planejei.exe"
!define UNINST   "Software\Microsoft\Windows\CurrentVersion\Uninstall\Planejei"
!define REGKEY   "Software\Planejei"

Name "${APP}"
OutFile "${OUT}"
InstallDir "$LOCALAPPDATA\Programs\${APP}"
InstallDirRegKey HKCU "${REGKEY}" "InstallDir"
BrandingText "${APP} ${VERSION}"

VIProductVersion "${VERSION}.0"
VIAddVersionKey /LANG=0 "ProductName" "${APP}"
VIAddVersionKey /LANG=0 "FileDescription" "Instalador do ${APP}"
VIAddVersionKey /LANG=0 "FileVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "ProductVersion" "${VERSION}"
VIAddVersionKey /LANG=0 "LegalCopyright" "Copyright (C) 2026 Lucas Bernardo. GPL-3.0-or-later"

!include "MUI2.nsh"
!include "FileFunc.nsh"

!define MUI_ICON   "..\assets\icon.ico"
!define MUI_UNICON "..\assets\icon.ico"
!define MUI_ABORTWARNING

!define MUI_FINISHPAGE_RUN "$INSTDIR\${EXE}"
!define MUI_FINISHPAGE_RUN_TEXT "Abrir o ${APP}"

!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_LICENSE "..\LICENSE"
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "PortugueseBR"

; O Planejei precisa estar fechado para o arquivo ser trocado.
!macro CLOSE_APP
  nsExec::Exec 'taskkill /IM "${EXE}" /F'
  Pop $0
  Sleep 300
!macroend

Section "Planejei (obrigatório)" SecApp
  SectionIn RO
  !insertmacro CLOSE_APP
  SetOutPath "$INSTDIR"
  File "${SRC}\${EXE}"
  File "${SRC}\LICENSE.txt"
  File "${SRC}\THIRD_PARTY.md"

  WriteUninstaller "$INSTDIR\Desinstalar.exe"

  CreateDirectory "$SMPROGRAMS\${APP}"
  CreateShortcut "$SMPROGRAMS\${APP}\${APP}.lnk" "$INSTDIR\${EXE}"
  CreateShortcut "$SMPROGRAMS\${APP}\Desinstalar o ${APP}.lnk" "$INSTDIR\Desinstalar.exe"

  ; Arquivos .planejei abrem no Planejei (com o ícone dele).
  WriteRegStr HKCU "Software\Classes\.planejei" "" "Planejei.Projeto"
  WriteRegStr HKCU "Software\Classes\Planejei.Projeto" "" "Projeto do Planejei"
  WriteRegStr HKCU "Software\Classes\Planejei.Projeto\DefaultIcon" "" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "Software\Classes\Planejei.Projeto\shell\open\command" "" '"$INSTDIR\${EXE}" "%1"'
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  ; "Aplicativos instalados" do Windows.
  WriteRegStr HKCU "${REGKEY}" "InstallDir" "$INSTDIR"
  WriteRegStr HKCU "${REGKEY}" "Version" "${VERSION}"
  WriteRegStr HKCU "${UNINST}" "DisplayName" "${APP}"
  WriteRegStr HKCU "${UNINST}" "DisplayVersion" "${VERSION}"
  WriteRegStr HKCU "${UNINST}" "Publisher" "${APP}"
  WriteRegStr HKCU "${UNINST}" "DisplayIcon" "$INSTDIR\${EXE},0"
  WriteRegStr HKCU "${UNINST}" "InstallLocation" "$INSTDIR"
  WriteRegStr HKCU "${UNINST}" "UninstallString" '"$INSTDIR\Desinstalar.exe"'
  WriteRegStr HKCU "${UNINST}" "QuietUninstallString" '"$INSTDIR\Desinstalar.exe" /S'
  WriteRegDWORD HKCU "${UNINST}" "NoModify" 1
  WriteRegDWORD HKCU "${UNINST}" "NoRepair" 1
  ${GetSize} "$INSTDIR" "/S=0K" $0 $1 $2
  WriteRegDWORD HKCU "${UNINST}" "EstimatedSize" $0
SectionEnd

Section "Atalho na área de trabalho" SecDesktop
  CreateShortcut "$DESKTOP\${APP}.lnk" "$INSTDIR\${EXE}"
SectionEnd

!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SecApp} "O Planejei e a associação dos arquivos .planejei."
  !insertmacro MUI_DESCRIPTION_TEXT ${SecDesktop} "Cria um atalho do Planejei na área de trabalho."
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  !insertmacro CLOSE_APP
  Delete "$INSTDIR\${EXE}"
  Delete "$INSTDIR\LICENSE.txt"
  Delete "$INSTDIR\THIRD_PARTY.md"
  Delete "$INSTDIR\Desinstalar.exe"
  RMDir "$INSTDIR"

  Delete "$DESKTOP\${APP}.lnk"
  RMDir /r "$SMPROGRAMS\${APP}"

  ; Só remove a associação se ela ainda for nossa.
  ReadRegStr $0 HKCU "Software\Classes\.planejei" ""
  StrCmp $0 "Planejei.Projeto" 0 +2
    DeleteRegKey HKCU "Software\Classes\.planejei"
  DeleteRegKey HKCU "Software\Classes\Planejei.Projeto"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'

  DeleteRegKey HKCU "${UNINST}"
  DeleteRegKey HKCU "${REGKEY}"
  ; Os projetos do usuário ficam onde ele salvou.
SectionEnd
