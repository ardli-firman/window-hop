; ==============================================================================
; Window Hop - NSIS Modern Installer Script
; ==============================================================================
; Targets Windows 10 & 11 64-bit per-user consistent installation without UAC.
; Bundles window-hop.exe, creates Start Menu & Desktop shortcuts, registers
; Windows Uninstaller, and supports interactive GUI & silent deployment.
; ==============================================================================

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

!ifndef VERSION
  !define VERSION "0.1.2"
!endif

!ifndef OUTFILE
  !define OUTFILE "..\dist\WindowHop-Setup.exe"
!endif

Name "Window Hop"
OutFile "${OUTFILE}"
InstallDir "$LOCALAPPDATA\Programs\Window Hop"
RequestExecutionLevel user

; General UI Configuration
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

; Installer Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES

; Finish page with launch option
!define MUI_FINISHPAGE_RUN "$INSTDIR\window-hop.exe"
!define MUI_FINISHPAGE_RUN_TEXT "Jalankan Window Hop sekarang"
!insertmacro MUI_PAGE_FINISH

; Uninstaller Pages
!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "Indonesian"
!insertmacro MUI_LANGUAGE "English"

Section "Install"
    ; 1. Verifikasi Arsitektur Sistem Operasi (Wajib 64-bit x64 sesuai ADR 0002)
    ${IfNot} ${IsNativeAMD64}
        MessageBox MB_ICONSTOP|MB_OK "Arsitektur OS tidak didukung. Window Hop memerlukan sistem operasi Windows 64-bit (x64)."
        Abort
    ${EndIf}

    ; 2. Siapkan direktori instalasi konsisten per-pengguna
    SetOutPath "$INSTDIR"

    ; 3. Salin berkas executable utama dan dokumen panduan sistem
    File "..\dist\window-hop.exe"
    File "..\dist\INSTALL-GUIDE.txt"

    ; 4. Tulis manifest instalasi (install-manifest.json)
    FileOpen $0 "$INSTDIR\install-manifest.json" w
    FileWrite $0 '{\n'
    FileWrite $0 '  "app_name": "Window Hop",\n'
    FileWrite $0 '  "version": "${VERSION}",\n'
    FileWrite $0 '  "installed_exe": "$INSTDIR\\window-hop.exe",\n'
    FileWrite $0 '  "installed_files": [\n'
    FileWrite $0 '    "$INSTDIR\\window-hop.exe",\n'
    FileWrite $0 '    "$INSTDIR\\INSTALL-GUIDE.txt",\n'
    FileWrite $0 '    "$INSTDIR\\install-manifest.json",\n'
    FileWrite $0 '    "$INSTDIR\\uninstall.exe"\n'
    FileWrite $0 '  ],\n'
    FileWrite $0 '  "shortcuts_created": [\n'
    FileWrite $0 '    "$SMPROGRAMS\\Window Hop\\Window Hop.lnk",\n'
    FileWrite $0 '    "$SMPROGRAMS\\Window Hop\\Uninstall Window Hop.lnk",\n'
    FileWrite $0 '    "$DESKTOP\\Window Hop.lnk"\n'
    FileWrite $0 '  ]\n'
    FileWrite $0 '}\n'
    FileClose $0

    ; 5. Buat Shortcut pada Start Menu
    CreateDirectory "$SMPROGRAMS\Window Hop"
    CreateShortcut "$SMPROGRAMS\Window Hop\Window Hop.lnk" "$INSTDIR\window-hop.exe" "" "$INSTDIR\window-hop.exe" 0
    CreateShortcut "$SMPROGRAMS\Window Hop\Uninstall Window Hop.lnk" "$INSTDIR\uninstall.exe"

    ; 6. Buat Shortcut pada Desktop Pengguna
    CreateShortcut "$DESKTOP\Window Hop.lnk" "$INSTDIR\window-hop.exe" "" "$INSTDIR\window-hop.exe" 0

    ; 7. Buat uninstaller mandiri
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; 8. Daftarkan entri Windows Add/Remove Programs (HKCU per-user)
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayName" "Window Hop"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayVersion" "${VERSION}"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "Publisher" "Window Hop Contributors"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "InstallLocation" "$INSTDIR"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayIcon" '"$INSTDIR\window-hop.exe",0'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "QuietUninstallString" '"$INSTDIR\uninstall.exe" /S'
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "NoModify" 1
    WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "NoRepair" 1
SectionEnd

Section "Uninstall"
    ; 1. Hentikan instance Window Hop jika sedang aktif
    DetailPrint "Menghentikan Window Hop jika sedang berjalan..."
    ExecWait 'taskkill /F /IM window-hop.exe'

    ; 2. Hapus registrasi startup jika aktif
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindowHop"

    ; 3. Hapus seluruh shortcut Start Menu dan Desktop
    Delete "$SMPROGRAMS\Window Hop\Window Hop.lnk"
    Delete "$SMPROGRAMS\Window Hop\Uninstall Window Hop.lnk"
    RMDir "$SMPROGRAMS\Window Hop"
    Delete "$DESKTOP\Window Hop.lnk"

    ; 4. Hapus berkas-berkas terpasang
    Delete "$INSTDIR\window-hop.exe"
    Delete "$INSTDIR\INSTALL-GUIDE.txt"
    Delete "$INSTDIR\install-manifest.json"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; 5. Hapus entri registry Windows Add/Remove Programs
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop"
SectionEnd
