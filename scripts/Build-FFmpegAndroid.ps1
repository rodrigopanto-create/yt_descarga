param(
    [string] $NdkPath = $env:ANDROID_NDK_HOME,
    [string] $BashPath = "$env:ProgramFiles\Git\bin\bash.exe"
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $NdkPath) {
    throw 'Configure ANDROID_NDK_HOME o indique -NdkPath.'
}
foreach ($required in @(
    $BashPath,
    (Join-Path $NdkPath 'toolchains\llvm\prebuilt\windows-x86_64\bin\aarch64-linux-android21-clang.cmd'),
    (Join-Path $NdkPath 'prebuilt\windows-x86_64\bin\make.exe')
)) {
    if (-not (Test-Path -LiteralPath $required -PathType Leaf)) {
        throw "Falta una herramienta de compilacion: $required"
    }
}

$work = Join-Path $repoRoot 'target\ffmpeg-android'
New-Item -ItemType Directory -Force -Path $work | Out-Null
$sources = @(
    @{
        Name = 'ffmpeg-8.0.3.tar.xz'
        Url = 'https://ffmpeg.org/releases/ffmpeg-8.0.3.tar.xz'
        Hash = '6136812ea6d4e68bdba27e33c2a94382711cdf4f8602ffef056ff792bd6f9818'
        Directory = 'ffmpeg-8.0.3'
    },
    @{
        Name = 'lame-3.100.tar.gz'
        Url = 'https://downloads.sourceforge.net/project/lame/lame/3.100/lame-3.100.tar.gz'
        Hash = 'ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e'
        Directory = 'lame-3.100'
    }
)
foreach ($source in $sources) {
    $archive = Join-Path $work $source.Name
    if (-not (Test-Path -LiteralPath $archive -PathType Leaf)) {
        & curl.exe --fail --location --retry 2 $source.Url --output $archive
        if ($LASTEXITCODE -ne 0) {
            throw "No se pudo descargar $($source.Name)"
        }
    }
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash -ne $source.Hash) {
        throw "SHA-256 incorrecto en $archive. No se compilara este archivo."
    }
    if (-not (Test-Path -LiteralPath (Join-Path $work $source.Directory))) {
        & $BashPath -c 'set -e; tar -xf "$1" -C "$2"' -- $archive $work
        if ($LASTEXITCODE -ne 0) {
            throw "No se pudo extraer $archive"
        }
    }
}

$vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
if (-not (Test-Path -LiteralPath $vswhere -PathType Leaf)) {
    throw 'Se requieren Visual Studio C++ Build Tools para las utilidades de compilacion host.'
}
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath
if (-not $visualStudio) {
    throw 'No se encontro una instalacion de Visual Studio con herramientas C++.'
}
$vcvars = Join-Path $visualStudio 'VC\Auxiliary\Build\vcvars64.bat'
$buildScript = Join-Path $repoRoot 'android\build_ffmpeg.sh'
$buildCommand = "call `"$vcvars`" >nul && `"$BashPath`" `"$buildScript`" `"$NdkPath`""
& $env:ComSpec /d /c $buildCommand
if ($LASTEXITCODE -ne 0) {
    throw 'Fallo la compilacion de FFmpeg/FFprobe para Android.'
}

$licenses = Join-Path $repoRoot 'assets\android-licenses'
New-Item -ItemType Directory -Force -Path $licenses | Out-Null
Copy-Item -LiteralPath (Join-Path $work 'ffmpeg-8.0.3\COPYING.LGPLv2.1') -Destination (Join-Path $licenses 'FFmpeg-LGPL-2.1.txt') -Force
Copy-Item -LiteralPath (Join-Path $work 'ffmpeg-8.0.3\LICENSE.md') -Destination (Join-Path $licenses 'FFmpeg-LICENSE.txt') -Force
Copy-Item -LiteralPath (Join-Path $work 'lame-3.100\COPYING') -Destination (Join-Path $licenses 'LAME-LGPL-2.txt') -Force

Write-Output 'FFmpeg y FFprobe Android ARM64 preparados en jniLibs\arm64-v8a.'
Write-Output 'Regenere el manifiesto con Generate-AndroidRuntimeManifest.ps1 -IncludeFfmpeg.'
