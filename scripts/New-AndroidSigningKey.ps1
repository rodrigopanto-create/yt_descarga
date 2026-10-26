param(
    [string] $KeystorePath = (Join-Path $env:USERPROFILE '.android\yt_descarga\release.p12'),
    [string] $KeyAlias = 'yt-descarga'
)

$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$KeystorePath = [IO.Path]::GetFullPath($KeystorePath)
if ($KeystorePath.StartsWith("$repoRoot\", [StringComparison]::OrdinalIgnoreCase)) {
    throw 'La clave privada debe guardarse fuera del repositorio.'
}
if (Test-Path -LiteralPath $KeystorePath) {
    throw "Ya existe $KeystorePath. No se sobrescribira ni reemplazara la clave."
}
if ([string]::IsNullOrWhiteSpace($KeyAlias)) {
    throw 'El alias no puede estar vacio.'
}

$javaHome = $env:JAVA_HOME
if (-not $javaHome) {
    $javaHome = Join-Path $env:ProgramFiles 'Android\Android Studio\jbr'
}
$keytool = Join-Path $javaHome 'bin\keytool.exe'
if (-not (Test-Path -LiteralPath $keytool -PathType Leaf)) {
    throw 'No se encontro keytool. Configure JAVA_HOME con un JDK.'
}

New-Item -ItemType Directory -Force -Path (Split-Path -Parent $KeystorePath) | Out-Null
Write-Host 'Cree una contrasena fuerte y guardela en su gestor de contrasenas, no en el chat.'
Write-Host 'keytool pedira la contrasena y su confirmacion; no se mostraran al escribir.'
& $keytool -genkeypair -keystore $KeystorePath -storetype PKCS12 `
    -alias $KeyAlias -keyalg RSA -keysize 3072 -sigalg SHA256withRSA `
    -validity 10000 -dname 'CN=yt_descarga'
if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $KeystorePath -PathType Leaf)) {
    throw 'No se pudo crear la clave. No se ha generado un APK de distribucion.'
}
Write-Host "Clave creada: $KeystorePath (alias: $KeyAlias)"
Write-Warning 'Haga una copia de seguridad cifrada de la clave y guarde la contrasena por separado. Perderlas impide firmar actualizaciones.'
