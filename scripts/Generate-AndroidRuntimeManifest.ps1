param(
    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[A-Za-z0-9._-]+$')]
    [string] $RuntimeVersion,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^3\.[0-9]+\.[0-9]+$')]
    [string] $PythonVersion,

    [Parameter(Mandatory = $true)]
    [ValidatePattern('^[0-9]{4}\.[0-9]{1,2}\.[0-9]{1,2}$')]
    [string] $YtDlpVersion,

    [switch] $IncludeFfmpeg
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$pythonExecutable = Join-Path $repoRoot 'jniLibs\arm64-v8a\libpython3.so'
$pythonAssets = Join-Path $repoRoot 'assets\android-runtime\python'
$ytDlpZipapp = Join-Path $repoRoot 'assets\android-runtime\yt-dlp.pyz'
$ffmpegExecutable = Join-Path $repoRoot 'jniLibs\arm64-v8a\libffmpeg.so'
$manifestPath = Join-Path $repoRoot 'assets\android-runtime\manifest.json'

foreach ($required in @($pythonExecutable, $pythonAssets, $ytDlpZipapp)) {
    if (-not (Test-Path -LiteralPath $required)) {
        throw "Falta el artefacto requerido: $required"
    }
}

$pythonMinor = "$($PythonVersion.Split('.')[0]).$($PythonVersion.Split('.')[1])"
$pythonStdlib = Join-Path $pythonAssets "lib\python$pythonMinor"
$requiredPythonPaths = @(
    (Join-Path $pythonStdlib 'os.py'),
    (Join-Path $pythonStdlib 'encodings\__init__.py'),
    (Join-Path $pythonStdlib 'lib-dynload')
)
foreach ($requiredPath in $requiredPythonPaths) {
    if (-not (Test-Path -LiteralPath $requiredPath)) {
        throw "Falta un componente de biblioteca estándar CPython expandida: $requiredPath"
    }
}

if (-not (Get-ChildItem -LiteralPath (Join-Path $pythonStdlib 'lib-dynload') -File -Filter '*.so' | Select-Object -First 1)) {
    throw "No se encontraron módulos nativos .so en $(Join-Path $pythonStdlib 'lib-dynload')"
}

$pythonAssetsPrefix = [IO.Path]::GetFullPath($pythonAssets).TrimEnd('\') + '\'
$files = @(
    Get-ChildItem -LiteralPath $pythonAssets -File -Recurse | ForEach-Object {
        $relative = $_.FullName.Substring($pythonAssetsPrefix.Length).Replace('\', '/')
        [ordered]@{
            asset = "android-runtime/python/$relative"
            install_path = $relative
            sha256 = (Get-FileHash -LiteralPath $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
        }
    }
)

if ($files.Count -eq 0) {
    throw "No hay archivos de runtime en $pythonAssets"
}

$ffmpegHash = $null
if ($IncludeFfmpeg) {
    if (-not (Test-Path -LiteralPath $ffmpegExecutable -PathType Leaf)) {
        throw "Se solicitó incluir ffmpeg pero no existe: $ffmpegExecutable"
    }
    $ffmpegHash = (Get-FileHash -LiteralPath $ffmpegExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
}

$manifest = [ordered]@{
    schema_version = 1
    runtime_version = $RuntimeVersion
    python_version = $PythonVersion
    python_executable_sha256 = (Get-FileHash -LiteralPath $pythonExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    ytdlp_version = $YtDlpVersion
    ytdlp_sha256 = (Get-FileHash -LiteralPath $ytDlpZipapp -Algorithm SHA256).Hash.ToLowerInvariant()
    python_files = $files
    ffmpeg_sha256 = $ffmpegHash
}

$json = $manifest | ConvertTo-Json -Depth 8
$manifestDir = Split-Path -Parent $manifestPath
New-Item -ItemType Directory -Force -Path $manifestDir | Out-Null
$tempPath = "$manifestPath.$PID.tmp"
try {
    [IO.File]::WriteAllText($tempPath, $json + [Environment]::NewLine, [Text.UTF8Encoding]::new($false))
    Move-Item -LiteralPath $tempPath -Destination $manifestPath -Force
} finally {
    if (Test-Path -LiteralPath $tempPath) {
        Remove-Item -LiteralPath $tempPath -Force
    }
}

Write-Output "Manifiesto generado: $manifestPath"
Write-Output "Archivos Python incluidos: $($files.Count)"
