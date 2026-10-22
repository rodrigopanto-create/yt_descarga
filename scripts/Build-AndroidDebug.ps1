param([switch] $IncludeFfmpeg)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$previousJava = $env:JAVA_HOME
$previousPath = $env:PATH
$previousIgnore = $env:ANDROID_AAPT_IGNORE

try {
    if (-not $env:JAVA_HOME) {
        $studioJava = Join-Path $env:ProgramFiles 'Android\Android Studio\jbr'
        if (-not (Test-Path -LiteralPath (Join-Path $studioJava 'bin\java.exe'))) {
            throw 'Configura JAVA_HOME con un JDK antes de compilar el APK.'
        }
        $env:JAVA_HOME = $studioJava
    }
    $env:PATH = (Join-Path $env:JAVA_HOME 'bin') + ';' + $env:PATH
    # AAPT omite directorios _* por defecto, incluidos paquetes necesarios de CPython.
    $env:ANDROID_AAPT_IGNORE = '!.git:!.svn:!CVS:!*.scc:.*:!*~'
    $libraries = @(
        'libpython3.so', 'libpython3.15.so', 'libcrypto_python.so',
        'libssl_python.so', 'libsqlite3_python.so'
    )
    if ($IncludeFfmpeg) {
        $libraries += @('libffmpeg.so', 'libffprobe.so')
    }
    $nativeDir = Join-Path $repoRoot 'target\android-runtime-libs\arm64-v8a'
    New-Item -ItemType Directory -Force -Path $nativeDir | Out-Null
    foreach ($file in Get-ChildItem -LiteralPath $nativeDir -File) {
        if ($file.Name -notin $libraries) {
            throw "Biblioteca no prevista en el staging Python: $($file.FullName)"
        }
    }
    foreach ($library in $libraries) {
        Copy-Item -LiteralPath (Join-Path $repoRoot "jniLibs\arm64-v8a\$library") -Destination $nativeDir
    }
    & (Join-Path $PSScriptRoot 'Generate-AndroidRuntimeManifest.ps1') `
        -RuntimeVersion cpython-3.15.0-android-arm64-v1 `
        -PythonVersion 3.15.0 -YtDlpVersion 2026.08.19 -IncludeFfmpeg:$IncludeFfmpeg

    Push-Location $repoRoot
    try {
        cargo apk build --lib --target aarch64-linux-android
        if ($LASTEXITCODE -ne 0) {
            throw "cargo apk build fallo con codigo $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repoRoot 'target' }
    $apkPath = Join-Path $targetDir 'debug\apk\yt_descarga_lib.apk'
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $archive = [IO.Compression.ZipFile]::OpenRead($apkPath)
    try {
        $manifest = Get-Content -LiteralPath (Join-Path $repoRoot 'assets\android-runtime\manifest.json') -Raw | ConvertFrom-Json
        $expected = @{}
        $expected['assets/android-runtime/manifest.json'] = (Get-FileHash -LiteralPath (Join-Path $repoRoot 'assets\android-runtime\manifest.json') -Algorithm SHA256).Hash
        $expected['assets/android-runtime/yt-dlp.pyz'] = $manifest.ytdlp_sha256
        foreach ($file in $manifest.python_files) {
            $expected["assets/$($file.asset)"] = $file.sha256
        }
        foreach ($library in $libraries) {
            $expected["lib/arm64-v8a/$library"] = (Get-FileHash -LiteralPath (Join-Path $nativeDir $library) -Algorithm SHA256).Hash
        }
        if ($IncludeFfmpeg) {
            foreach ($license in @('FFmpeg-LGPL-2.1.txt', 'LAME-LGPL.txt')) {
                $expected["assets/android-runtime/licenses/$license"] = (Get-FileHash -LiteralPath (Join-Path $repoRoot "assets\android-runtime\licenses\$license") -Algorithm SHA256).Hash
            }
        }
        foreach ($name in $expected.Keys) {
            $entry = $archive.GetEntry($name)
            if (-not $entry) {
                throw "El APK omitio un archivo requerido: $name"
            }
            $stream = $entry.Open()
            $sha = [Security.Cryptography.SHA256]::Create()
            try {
                $actual = [BitConverter]::ToString($sha.ComputeHash($stream)).Replace('-', '')
                if ($actual -ine $expected[$name]) {
                    throw "SHA-256 incorrecto dentro del APK: $name"
                }
            } finally {
                $sha.Dispose()
                $stream.Dispose()
            }
        }
        foreach ($entry in $archive.Entries) {
            if ($entry.FullName -match '^lib/arm64-v8a/(.+\.so)$' -and
                $Matches[1] -notin ($libraries + @('libyt_descarga_lib.so', 'libc++_shared.so'))) {
                throw "Biblioteca ajena al runtime Python en APK sin MPV: $($entry.FullName)"
            }
        }
        Write-Output "APK verificado: $apkPath"
        Write-Output "Archivos comprobados por SHA-256: $($expected.Count)"
    } finally {
        $archive.Dispose()
    }
} finally {
    $env:JAVA_HOME = $previousJava
    $env:PATH = $previousPath
    $env:ANDROID_AAPT_IGNORE = $previousIgnore
}
