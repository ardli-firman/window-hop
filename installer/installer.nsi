; Window Hop NSIS Installer Script
; Targets Windows 10 & 11 64-bit per-user consistent installation

!include "MUI2.nsh"
!include "LogicLib.nsh"
!include "x64.nsh"

Name "Window Hop"
OutFile "..\target\window-hop-setup.exe"
InstallDir "$LOCALAPPDATA\Programs\Window Hop"
RequestExecutionLevel user

; Interface Settings
!define MUI_ABORTWARNING
!define MUI_ICON "${NSISDIR}\Contrib\Graphics\Icons\modern-install.ico"
!define MUI_UNICON "${NSISDIR}\Contrib\Graphics\Icons\modern-uninstall.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

!insertmacro MUI_LANGUAGE "English"

Section "Install"
    ; 1. Verify 64-bit Windows OS
    ${IfNot} ${IsNativeAMD64}
        MessageBox MB_ICONSTOP|MB_OK "Window Hop memerlukan sistem operasi Windows 64-bit (x64)."
        Abort
    ${EndIf}

    ; 2. Create installation directory
    SetOutPath "$INSTDIR"

    ; 3. Copy executable and documentation
    File "..\target\release\window-hop.exe"
    File "..\docs\install-guide.md"

    ; 4. Create Start Menu Shortcuts
    CreateDirectory "$SMPROGRAMS\Window Hop"
    CreateShortcut "$SMPROGRAMS\Window Hop\Window Hop.lnk" "$INSTDIR\window-hop.exe" "" "$INSTDIR\window-hop.exe" 0
    CreateShortcut "$SMPROGRAMS\Window Hop\Uninstall Window Hop.lnk" "$INSTDIR\uninstall.exe"

    ; 5. Write uninstaller
    WriteUninstaller "$INSTDIR\uninstall.exe"

    ; 6. Register Add/Remove Programs
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayName" "Window Hop"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "UninstallString" '"$INSTDIR\uninstall.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayIcon" '"$INSTDIR\window-hop.exe"'
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "Publisher" "Window Hop Contributors"
    WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop" "DisplayVersion" "0.1.0"
SectionEnd

Section "Uninstall"
    ; 1. Unregister Startup Run key if set
    DeleteRegValue HKCU "Software\Microsoft\Windows\CurrentVersion\Run" "WindowHop"

    ; 2. Remove Shortcuts
    Delete "$SMPROGRAMS\Window Hop\Window Hop.lnk"
    Delete "$SMPROGRAMS\Window Hop\Uninstall Window Hop.lnk"
    RMDir "$SMPROGRAMS\Window Hop"

    ; 3. Remove Installed Files
    Delete "$INSTDIR\window-hop.exe"
    Delete "$INSTDIR\install-guide.md"
    Delete "$INSTDIR\INSTALL-GUIDE.txt"
    Delete "$INSTDIR\install-manifest.json"
    Delete "$INSTDIR\uninstall.exe"
    RMDir "$INSTDIR"

    ; 4. Unregister from Add/Remove Programs
    DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\WindowHop"
SectionEnd
