param(
    [switch] $Release,
    [switch] $DebugSigning,
    [string] $KeystorePath,
    [string] $KeyAlias = 'yt-descarga',
    [Security.SecureString] $KeystorePassword
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
if ($DebugSigning -and -not $Release) {
    throw '-DebugSigning requiere -Release; solo se usa para pruebas locales optimizadas.'
}
if (($KeystorePath -or $KeystorePassword) -and (-not $Release -or $DebugSigning)) {
    throw 'La clave de distribucion requiere -Release y no admite -DebugSigning.'
}
if ($Release -and -not $DebugSigning -and -not $KeystorePath) {
    $KeystorePath = $env:CARGO_APK_RELEASE_KEYSTORE
    if (-not $KeystorePath) {
        $KeystorePath = Join-Path $env:USERPROFILE '.android\yt_descarga\release.p12'
    }
}
if ($Release -and -not $DebugSigning) {
    $KeystorePath = [IO.Path]::GetFullPath($KeystorePath)
    if ($KeystorePath.StartsWith("$repoRoot\", [StringComparison]::OrdinalIgnoreCase)) {
        throw 'La clave privada debe guardarse fuera del repositorio.'
    }
    if (-not (Test-Path -LiteralPath $KeystorePath -PathType Leaf)) {
        throw 'No existe la clave de distribucion. Ejecute scripts\New-AndroidSigningKey.ps1 primero.'
    }
    if ([string]::IsNullOrWhiteSpace($KeyAlias)) {
        throw 'El alias no puede estar vacio.'
    }
}

$nativeSource = Join-Path $repoRoot 'jniLibs\arm64-v8a'
$nativeStage = Join-Path $repoRoot 'target\android-runtime-libs\arm64-v8a'
New-Item -ItemType Directory -Force -Path $nativeStage | Out-Null
Get-ChildItem -LiteralPath $nativeStage -File -Filter '*.so' | ForEach-Object {
    Remove-Item -LiteralPath $_.FullName -Force
}
$nativeLibraries = @(
    'libpython3.so', 'libpython3.15.so',
    'libssl_python.so', 'libcrypto_python.so', 'libsqlite3_python.so'
)
$runtimeManifest = Get-Content -LiteralPath (Join-Path $repoRoot 'assets\android-runtime\manifest.json') -Raw | ConvertFrom-Json
if ($runtimeManifest.ffmpeg_sha256 -or $runtimeManifest.ffprobe_sha256) {
    if (-not $runtimeManifest.ffmpeg_sha256 -or -not $runtimeManifest.ffprobe_sha256) {
        throw 'El manifiesto debe declarar FFmpeg y FFprobe juntos.'
    }
    $nativeLibraries += @('libffmpeg.so', 'libffprobe.so')
}
foreach ($name in $nativeLibraries) {
    $source = Join-Path $nativeSource $name
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Falta una biblioteca del runtime Android: $source"
    }
    Copy-Item -LiteralPath $source -Destination (Join-Path $nativeStage $name) -Force
}

if (-not $env:JAVA_HOME) {
    $androidStudioJbr = Join-Path $env:ProgramFiles 'Android\Android Studio\jbr'
    if (Test-Path (Join-Path $androidStudioJbr 'bin\java.exe')) {
        $env:JAVA_HOME = $androidStudioJbr
    }
}
if ($env:JAVA_HOME -and (Test-Path (Join-Path $env:JAVA_HOME 'bin\java.exe'))) {
    $env:PATH = "$(Join-Path $env:JAVA_HOME 'bin');$env:PATH"
}

# Keep CPython's underscore-prefixed standard-library directories in APK assets.
$env:ANDROID_AAPT_IGNORE = '!.svn:!.git:!.ds_store:!*.scc:.*:!CVS:!thumbs.db:!picasa.ini:!*~'

$cargoArgs = @('apk', 'build', '--target', 'aarch64-linux-android')
if ($Release) {
    $cargoArgs += '--release'
}

$previousKeystore = $env:CARGO_APK_RELEASE_KEYSTORE
$previousPassword = $env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD
$previousSigningPassword = $env:YT_DESCARGA_SIGNING_PASSWORD
$certificatePath = $null
$candidateApk = $null
$certificate = $null
try {
    if ($Release -and -not $DebugSigning) {
        $sdkRoot = $env:ANDROID_HOME
        if (-not $sdkRoot) { $sdkRoot = $env:ANDROID_SDK_ROOT }
        if (-not $sdkRoot) { $sdkRoot = Join-Path $env:LOCALAPPDATA 'Android\Sdk' }
        $buildTools = Get-ChildItem -LiteralPath (Join-Path $sdkRoot 'build-tools') -Directory |
            Where-Object { $_.Name -match '^\d+\.\d+\.\d+$' } |
            Sort-Object { [version] $_.Name } -Descending |
            Where-Object { Test-Path -LiteralPath (Join-Path $_.FullName 'lib\apksigner.jar') } |
            Select-Object -First 1
        if (-not $buildTools) { throw 'No se encontro apksigner en los Android SDK Build Tools.' }
        $apksignerJar = Join-Path $buildTools.FullName 'lib\apksigner.jar'
        $java = Join-Path $env:JAVA_HOME 'bin\java.exe'
        $keytool = Join-Path $env:JAVA_HOME 'bin\keytool.exe'
        if (-not (Test-Path -LiteralPath $java) -or -not (Test-Path -LiteralPath $keytool)) {
            throw 'Configure JAVA_HOME con un JDK que contenga java y keytool.'
        }
        if (-not $KeystorePassword -and -not $previousPassword) {
            $KeystorePassword = Read-Host 'Contrasena de la clave de distribucion' -AsSecureString
        }
        if ($KeystorePassword) {
            $passwordPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($KeystorePassword)
            try {
                $env:YT_DESCARGA_SIGNING_PASSWORD = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($passwordPointer)
            } finally {
                [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($passwordPointer)
            }
        } else {
            $env:YT_DESCARGA_SIGNING_PASSWORD = $previousPassword
        }
        if ([string]::IsNullOrEmpty($env:YT_DESCARGA_SIGNING_PASSWORD)) {
            throw 'La contrasena de distribucion no puede estar vacia.'
        }
        $certificatePath = Join-Path $repoRoot "target\signing-$([guid]::NewGuid()).cer"
        & $keytool -exportcert -keystore $KeystorePath -alias $KeyAlias `
            -storepass:env YT_DESCARGA_SIGNING_PASSWORD -file $certificatePath
        if ($LASTEXITCODE -ne 0) {
            throw 'No se pudo abrir la clave. Compruebe la contrasena y el alias.'
        }
        $expectedDigest = (Get-FileHash -LiteralPath $certificatePath -Algorithm SHA256).Hash.ToLowerInvariant()
        $certificate = New-Object Security.Cryptography.X509Certificates.X509Certificate2($certificatePath)
        if ($certificate.Subject -match '(^|,\s*)CN=Android Debug(,|$)' -or $certificate.NotAfter -le (Get-Date)) {
            throw 'El certificado es debug o esta vencido; no se permite distribuirlo.'
        }
    }
    if ($Release) {
        $debugKeystore = Join-Path $env:USERPROFILE '.android\debug.keystore'
        if (-not (Test-Path -LiteralPath $debugKeystore -PathType Leaf)) {
            throw "No existe el keystore debug local: $debugKeystore"
        }
        if ($DebugSigning) {
            Write-Warning 'APK optimizado firmado con clave DEBUG: no distribuir como release.'
        }
        # cargo-apk 0.10 passes passwords in argv; only use its public debug password.
        $env:CARGO_APK_RELEASE_KEYSTORE = $debugKeystore
        $env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = 'android'
    }
    Push-Location $repoRoot
    try {
        & cargo @cargoArgs
        if ($LASTEXITCODE -ne 0) {
            throw "cargo-apk termino con codigo $LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }
    if ($Release -and -not $DebugSigning) {
        $distribution = Join-Path $repoRoot 'target\release\distribution'
        New-Item -ItemType Directory -Force -Path $distribution | Out-Null
        $candidateApk = Join-Path $distribution "signing-$([guid]::NewGuid()).apk"
        $sourceApk = Join-Path $repoRoot 'target\release\apk\yt_descarga_lib.apk'
        & $java -jar $apksignerJar sign --ks $KeystorePath --ks-key-alias $KeyAlias `
            --ks-pass env:YT_DESCARGA_SIGNING_PASSWORD --key-pass env:YT_DESCARGA_SIGNING_PASSWORD `
            --v1-signing-enabled true --v2-signing-enabled true --v3-signing-enabled true `
            --v4-signing-enabled false --out $candidateApk $sourceApk
        if ($LASTEXITCODE -ne 0) { throw 'Fallo la firma de distribucion.' }
        $verification = & $java -jar $apksignerJar verify --verbose --print-certs $candidateApk
        if ($LASTEXITCODE -ne 0) { throw 'La firma del APK no es valida.' }
        $digests = @($verification | Select-String '^Signer #\d+ certificate SHA-256 digest: (\w+)$' |
            ForEach-Object { $_.Matches[0].Groups[1].Value.ToLowerInvariant() })
        if ($digests.Count -ne 1 -or $digests[0] -ne $expectedDigest) {
            throw 'El certificado del APK no coincide con la clave de distribucion.'
        }
        $apk = Join-Path $distribution 'yt_descarga.apk'
        Move-Item -LiteralPath $candidateApk -Destination $apk -Force
        $candidateApk = $null
        $verification | ForEach-Object { Write-Host $_ }
        Write-Host "APK de distribucion verificado: $apk"
        Write-Host "Certificado SHA-256: $expectedDigest"
        Write-Host "APK SHA-256: $((Get-FileHash -LiteralPath $apk -Algorithm SHA256).Hash)"
    }
} finally {
    $env:CARGO_APK_RELEASE_KEYSTORE = $previousKeystore
    $env:CARGO_APK_RELEASE_KEYSTORE_PASSWORD = $previousPassword
    $env:YT_DESCARGA_SIGNING_PASSWORD = $previousSigningPassword
    if ($certificate) { $certificate.Dispose() }
    foreach ($temporary in @($certificatePath, $candidateApk)) {
        if ($temporary -and (Test-Path -LiteralPath $temporary)) {
            Remove-Item -LiteralPath $temporary -Force
        }
    }
}
