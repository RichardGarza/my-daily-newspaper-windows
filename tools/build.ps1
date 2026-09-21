# Builds My Daily Newspaper on Windows. Start it by double-clicking
# "Build My Daily Newspaper.bat" in the project folder.
#
# What it does, in order:
#   1. checks the things a build needs and offers to install what's missing
#      (Microsoft C++ build tools, Rust, Node.js, the WebView2 runtime) with winget
#   2. npm install
#   3. asks your first name (once) and draws your icon
#   4. compiles the app (first time: 5-10 minutes)
#   5. installs it for you only (no admin): %LOCALAPPDATA%\Programs\My Daily Newspaper
#      with Start Menu and Desktop shortcuts, and opens it
#
# Nothing here touches your Claude or Grok logins. The app talks to the
# `claude` command that is already signed in on this PC.
#
# Keep this file plain ASCII: Windows PowerShell 5.1 misreads anything else.

param([string]$Name = "")

$ErrorActionPreference = "Continue"
$ProjectDir = Split-Path -Parent $PSScriptRoot
Set-Location -LiteralPath $ProjectDir

if (-not (Test-Path "package.json") -or -not (Test-Path "src-tauri")) {
    Write-Host "Can't find the project files next to this script. Run the .bat from inside the project folder."
    Read-Host "Press Enter to close"
    exit 1
}

try { Stop-Transcript | Out-Null } catch {}
Start-Transcript -Path (Join-Path $ProjectDir "build.log") -Force | Out-Null
Write-Host "Build started $(Get-Date)"
Write-Host "Windows $([Environment]::OSVersion.Version)  PowerShell $($PSVersionTable.PSVersion)"

function Step([string]$text) {
    Write-Host ""
    Write-Host "== $text" -ForegroundColor White
}

function Fail([string]$message) {
    Write-Host ""
    Write-Host "Stopped: $message" -ForegroundColor Red
    Write-Host "The details are saved in build.log, in the project folder."
    Write-Host ""
    try { Stop-Transcript | Out-Null } catch {}
    Read-Host "Press Enter to close"
    exit 1
}

function Have([string]$command) {
    return [bool](Get-Command $command -ErrorAction SilentlyContinue)
}

# Programs installed a moment ago aren't on this window's PATH yet.
function Update-Path {
    $machine = [Environment]::GetEnvironmentVariable("Path", "Machine")
    $user = [Environment]::GetEnvironmentVariable("Path", "User")
    $extra = @("$env:USERPROFILE\.cargo\bin", "$env:ProgramFiles\nodejs", "$env:APPDATA\npm", "$env:USERPROFILE\.local\bin")
    $env:Path = (@($machine, $user) + $extra | Where-Object { $_ }) -join ";"
}

function Ask-Yes([string]$question) {
    $answer = Read-Host "$question [y/N]"
    return ($answer -match '^\s*[yY]')
}

function Install-WithWinget([string]$id, [string]$what, [string]$override = "") {
    if (-not (Have "winget")) {
        Fail "$what is missing and winget isn't available to install it. Install $what by hand (see the README), then run me again."
    }
    Write-Host "Installing $what with winget. Windows may ask for permission."
    $arguments = @("install", "--id", $id, "-e", "--accept-package-agreements", "--accept-source-agreements")
    if ($override) { $arguments += @("--override", $override) }
    & winget @arguments
    if ($LASTEXITCODE -ne 0) {
        Write-Host "winget finished with code $LASTEXITCODE (that can also mean 'already installed')." -ForegroundColor Yellow
    }
    Update-Path
}

# Runs a command line through cmd.exe so normal output and error output arrive
# as one plain stream: it shows in the window and lands in build.log intact.
function Invoke-Logged([string]$commandLine) {
    & cmd.exe /d /c "$commandLine 2>&1" | ForEach-Object { Write-Host $_ }
    return $LASTEXITCODE
}

Update-Path

# ---------------------------------------------------------------- 1. tools
Step "1/6  Microsoft C++ build tools"
$vswhere = Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"
function Find-VcTools {
    if (-not (Test-Path $vswhere)) { return $null }
    $found = & $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
    if ($found) { return ($found | Select-Object -First 1) }
    return $null
}
$vc = Find-VcTools
if (-not $vc) {
    Write-Host "Rust needs Microsoft's C++ build tools to link the app. They aren't installed."
    Write-Host "It's a big one-time download (several GB, 10-25 minutes)."
    if (Ask-Yes "Install them now with winget?") {
        Install-WithWinget "Microsoft.VisualStudio.2022.BuildTools" "the C++ build tools" "--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
        $vc = Find-VcTools
    }
    if (-not $vc) {
        Fail "The C++ build tools are still missing. Install 'Build Tools for Visual Studio' from https://visualstudio.microsoft.com/downloads/ and tick 'Desktop development with C++', then run me again."
    }
}
Write-Host "ok: $vc"

Step "2/6  Rust, Node.js, WebView2"
if (-not (Have "cargo")) {
    Write-Host "Rust isn't installed."
    if (Ask-Yes "Install Rust now with winget (one time, about 5 minutes)?") {
        Install-WithWinget "Rustlang.Rustup" "Rust"
    }
    if (-not (Have "cargo")) { Fail "Rust is still missing. Install it from https://rustup.rs and run me again." }
}
$rustHost = "$(& rustc -vV 2>$null | Select-String '^host:')"
Write-Host "$(& cargo --version)  ($rustHost)"
if ($rustHost -notmatch "msvc") {
    Write-Host "This Rust targets '$rustHost'. The app needs the MSVC flavour; switching the default." -ForegroundColor Yellow
    & rustup default stable-x86_64-pc-windows-msvc
    if ($LASTEXITCODE -ne 0) { Fail "Couldn't switch Rust to stable-x86_64-pc-windows-msvc." }
}

$nodeOk = $false
if (Have "node") {
    $major = 0
    [void][int]::TryParse((& node -p "process.versions.node.split('.')[0]"), [ref]$major)
    $nodeOk = ($major -ge 20)
    if (-not $nodeOk) { Write-Host "Node.js $(& node -v) is too old (need 20 or newer)." }
}
if (-not $nodeOk) {
    if (Ask-Yes "Install Node.js LTS now with winget?") {
        Install-WithWinget "OpenJS.NodeJS.LTS" "Node.js"
    }
    if (-not (Have "node")) { Fail "Node.js is still missing. Install version 20 or newer from https://nodejs.org and run me again." }
}
Write-Host "node $(& node -v)"

$webview = $false
foreach ($key in @(
        "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}",
        "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}")) {
    $pv = (Get-ItemProperty -Path $key -Name pv -ErrorAction SilentlyContinue).pv
    if ($pv -and $pv -ne "0.0.0.0") { $webview = $true; Write-Host "WebView2 runtime $pv"; break }
}
if (-not $webview) {
    Write-Host "The WebView2 runtime (what the app draws its window with) isn't installed."
    if (Ask-Yes "Install it now with winget?") {
        Install-WithWinget "Microsoft.EdgeWebView2Runtime" "the WebView2 runtime"
    } else {
        Write-Host "Carrying on. The app won't open without it: https://developer.microsoft.com/microsoft-edge/webview2/" -ForegroundColor Yellow
    }
}

# ------------------------------------------------------------ 2. packages
Step "3/6  JavaScript packages"
$code = Invoke-Logged "npm install --no-audit --no-fund"
if ($code -ne 0) { Fail "npm install failed." }
# npm sometimes skips the Windows-specific native pieces of the build tools
# when the lockfile was made on another system. If the bundler can't start,
# wipe and reinstall once from scratch.
& cmd.exe /d /c "npx --no-install vite --version >nul 2>&1"
$viteOk = ($LASTEXITCODE -eq 0)
& cmd.exe /d /c "npx --no-install tauri --version >nul 2>&1"
$tauriOk = ($LASTEXITCODE -eq 0)
if (-not ($viteOk -and $tauriOk)) {
    Write-Host "Build tools didn't install cleanly - reinstalling from scratch..."
    Remove-Item -Recurse -Force "node_modules" -ErrorAction SilentlyContinue
    Remove-Item -Force "package-lock.json" -ErrorAction SilentlyContinue
    $code = Invoke-Logged "npm install --no-audit --no-fund"
    if ($code -ne 0) { Fail "npm install failed (second try)." }
    & cmd.exe /d /c "npx --no-install vite --version >nul 2>&1"
    if ($LASTEXITCODE -ne 0) { Fail "The bundler (vite) still won't start. Node version: $(& node -v)" }
}

# ---------------------------------------------------------------- 3. name
Step "4/6  Whose paper is this?"
# Remembered in .owner (never committed). Delete that file to be asked again,
# or pass a name:  "Build My Daily Newspaper.bat" Priya
function Clean-Name([string]$raw) {
    if (-not $raw) { return "" }
    $clean = ($raw -replace '[\x00-\x1f"\\<>&%!^|]', '').Trim()
    if ($clean.Length -gt 40) { $clean = $clean.Substring(0, 40).Trim() }
    return $clean
}
$owner = Clean-Name $Name
if (-not $owner -and (Test-Path ".owner")) {
    $owner = Clean-Name (Get-Content ".owner" -TotalCount 1 -Encoding UTF8)
}
if (-not $owner) {
    Write-Host "Your first name goes on the masthead (""Sam's Daily"") and your initial on the icon."
    $owner = Clean-Name (Read-Host "Your first name (or just Enter to skip)")
}
if ($owner) {
    [IO.File]::WriteAllText((Join-Path $ProjectDir ".owner"), $owner + "`n", (New-Object Text.UTF8Encoding($false)))
    Write-Host "Building $owner's Daily."
} else {
    Write-Host "No name given - the masthead will say ""My Daily"" until you set one in the app."
}
$env:DAILY_OWNER_NAME = $owner

# The icon maker reads the name from the environment: Windows PowerShell drops
# empty arguments and mangles some characters on the way to a program.
$env:ICON_MAKER_NAME = $owner
$code = Invoke-Logged "cargo run --quiet --release --manifest-path tools\icon-maker\Cargo.toml -- - src-tauri\icons-personal"
if ($code -ne 0) { Fail "Couldn't draw the icon." }

# ------------------------------------------------------------- 4. compile
Step "5/6  Compiling My Daily Newspaper (first time: 5-10 minutes)"
$code = Invoke-Logged "npm run tauri build -- --no-bundle"
if ($code -ne 0) { Fail "The build failed." }

$built = Join-Path $ProjectDir "src-tauri\target\release\my-daily-newspaper.exe"
if (-not (Test-Path $built)) { Fail "Build finished but I can't find the app at: $built" }

# ------------------------------------------------------------- 5. install
Step "6/6  Installing"
$installDir = Join-Path $env:LOCALAPPDATA "Programs\My Daily Newspaper"
$exe = Join-Path $installDir "My Daily Newspaper.exe"
Get-Process -Name "My Daily Newspaper", "my-daily-newspaper" -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Sleep -Milliseconds 800
New-Item -ItemType Directory -Force -Path $installDir | Out-Null
try {
    Copy-Item -LiteralPath $built -Destination $exe -Force -ErrorAction Stop
} catch {
    Fail "Couldn't copy the app into $installDir ($($_.Exception.Message)). Close My Daily Newspaper if it's open and run me again."
}

$shell = New-Object -ComObject WScript.Shell
$startMenu = Join-Path $env:APPDATA "Microsoft\Windows\Start Menu\Programs"
$desktop = [Environment]::GetFolderPath("Desktop")
foreach ($folder in @($startMenu, $desktop)) {
    if (-not $folder -or -not (Test-Path $folder)) { continue }
    $link = $shell.CreateShortcut((Join-Path $folder "My Daily Newspaper.lnk"))
    $link.TargetPath = $exe
    $link.WorkingDirectory = $installDir
    $link.IconLocation = "$exe,0"
    $link.Description = "Your own daily newspaper"
    $link.Save()
}
Write-Host "Installed: $exe"
Write-Host "Shortcuts: Start Menu and Desktop."

# Paper delivery needs something that can send a PDF to a printer unattended.
$sumatra = @("$env:LOCALAPPDATA\SumatraPDF\SumatraPDF.exe", "$env:ProgramFiles\SumatraPDF\SumatraPDF.exe", "${env:ProgramFiles(x86)}\SumatraPDF\SumatraPDF.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $sumatra -and -not (Have "SumatraPDF")) {
    Write-Host ""
    Write-Host "Optional: automatic printing (the paper edition on your printer every morning)"
    Write-Host "uses SumatraPDF, a small free PDF reader. Everything else works without it."
    if ((Have "winget") -and (Ask-Yes "Install SumatraPDF now with winget?")) {
        Install-WithWinget "SumatraPDF.SumatraPDF" "SumatraPDF"
    }
}

Write-Host ""
Write-Host "Done." -ForegroundColor Green
if (-not (Have "claude")) {
    Write-Host "Heads up: the 'claude' command isn't on this PC yet. Without it the presses can't run." -ForegroundColor Yellow
    Write-Host "Install Claude Code from PowerShell:   irm https://claude.ai/install.ps1 | iex"
    Write-Host "then run 'claude' once and sign in with /login."
}
Write-Host "Opening it now..."
Start-Process -FilePath $exe -WorkingDirectory $installDir
try { Stop-Transcript | Out-Null } catch {}
Write-Host ""
Read-Host "Press Enter to close"
exit 0
