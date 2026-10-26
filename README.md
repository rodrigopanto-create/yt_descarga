# yt_descarga

Aplicación Android nativa escrita en Rust con interfaz `egui`. La aplicación descarga audio o video mediante `yt-dlp`, ejecutado en un hilo secundario, y muestra la salida en pantalla.

## Alcance actual

- El único destino de compilación de la aplicación es Android `arm64-v8a`.
- La entrada activa es la librería Android `yt_descarga_lib`; no se genera un ejecutable de escritorio ni un cliente WASM.
- La interfaz permite pegar una URL desde el portapapeles, elegir MP3 o MP4 y ver los mensajes de la descarga.
- Los permisos Android activos se declaran en `Cargo.toml` bajo `package.metadata.android.uses_permission`, porque `cargo-apk` genera el manifiesto del APK desde esa configuración.
- El runtime Android valida los hashes de CPython, yt-dlp y sus archivos antes de instalarlos en el directorio privado de la app.
- Las descargas se escriben primero en almacenamiento privado temporal. En Android 10 o posterior se publican en `Descargas/yt_descarga` mediante MediaStore; en versiones anteriores se guardan en el directorio externo privado de la app.

## Estado del runtime requerido

El launcher PIE de CPython está empaquetado como `jniLibs/arm64-v8a/libpython3.so` y el manifiesto se genera desde los archivos reales del runtime. El build debe conservar los directorios CPython que comienzan con `_`; usa el script de build documentado abajo. Se verificaron en Android ARM64 una descarga MP4, una descarga con conversión a MP3 y la publicación en Descargas. FFmpeg/FFprobe 8.0.3 también pasaron una prueba de unión de pistas separadas.

Consulta [`assets/README.md`](./assets/README.md) para conocer los requisitos, generar el manifiesto y verificar el runtime en un dispositivo. `ffmpeg` es opcional, pero se requiere para convertir a MP3 y combinar algunas descargas de video.

### Preparar FFmpeg y FFprobe

En Windows se usan Git Bash, Visual Studio C++ Build Tools y el NDK. No se requiere WSL.

```powershell
$env:ANDROID_NDK_HOME = "$env:LOCALAPPDATA\Android\Sdk\ndk\30.0.16248370"
.\scripts\Build-FFmpegAndroid.ps1
.\scripts\Generate-AndroidRuntimeManifest.ps1 `
  -RuntimeVersion cpython-3.15.0-android-arm64-v2 `
  -PythonVersion 3.15.0 -YtDlpVersion 2026.08.19 -IncludeFfmpeg
```

El script descarga fuentes con versiones y SHA-256 fijados y compila ejecutables PIE ARM64
`libffmpeg.so` y `libffprobe.so`, con LAME enlazado estáticamente para MP3. No usa las bibliotecas
antiguas de MPV. Las licencias se copian a los assets del APK; antes de distribuir los binarios,
proporcionar también las fuentes correspondientes y la receta de compilación descrita en
[`assets/README.md`](./assets/README.md).

## Compilar para Android

Requisitos: Rust, el target Android de Rust, Android NDK y `cargo-apk`.

```powershell
rustup target add aarch64-linux-android
.\scripts\Build-Android.ps1
```

El script configura `ANDROID_AAPT_IGNORE` para que CPython no pierda carpetas como `_pyrepl` al empaquetar los assets. Esto genera un APK de depuración. Para distribuir un APK firmado, sigue la sección siguiente. Para instalar y verificar el APK, usa un dispositivo o emulador ARM64 con la depuración habilitada.

El empaquetado selecciona solo las bibliotecas del runtime activo en
`target/android-runtime-libs/arm64-v8a`; las bibliotecas antiguas MPV/FFmpeg se conservan en
el repositorio, pero no se incluyen en el APK de descargas.

Para probar una compilación optimizada con la clave debug local:

```powershell
.\scripts\Build-Android.ps1 -Release -DebugSigning
```

Esta opción es solo para pruebas, no para distribución. Evita el aborto observado en el
arranque de builds debug en la combinación actual de Rust y `ndk`; no corrige esa dependencia.

### Firma de distribución (APK directo)

Ejecuta una sola vez en una terminal interactiva:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\New-AndroidSigningKey.ps1
```

Se crea una clave RSA de 3072 bits en `%USERPROFILE%\.android\yt_descarga\release.p12`,
fuera del repositorio, con alias `yt-descarga` y validez de 10.000 días. `keytool` pide
la contraseña y su confirmación sin mostrarlas. Guarda una contraseña fuerte en un gestor
y una copia de seguridad cifrada del archivo en otro lugar. **No recrees la clave para cada
versión: perderla o perder su contraseña impide publicar actualizaciones compatibles.**
El script rechaza archivos existentes y no utiliza el antiguo `release.keystore` del proyecto.

Para cada versión:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\Build-Android.ps1 -Release
```

La contraseña se solicita oculta en la terminal. El APK para compartir es
`target\release\distribution\yt_descarga.apk`, y solo se reemplaza después de verificar
su firma y que su certificado SHA-256 coincida con la clave elegida. La salida muestra
también el SHA-256 del APK. No compartas el archivo `.p12` ni su contraseña.

Certificado de distribución actual: `CN=yt_descarga`, RSA de 3072 bits. Su huella pública
SHA-256, verificada en el APK firmado, es:

```text
532a17fa19ebaac9bd91865a4a7fd36d00b3eb2f7a9c0db15cfed238fb6c0f34
```

En VS Code también están las tareas **Android: crear clave de distribucion (una sola vez)**
y **Android: APK de distribucion firmado**, accesibles desde **Terminal > Ejecutar tarea**.

`cargo-apk` 0.10 pasa contraseñas al firmador como argumentos; por ello compila un APK
intermedio con la clave debug local. **El archivo de `target\release\apk` no es el entregable
de distribución.** El script firma el APK final con `apksigner` (v1/v2/v3), pasando el secreto
mediante una variable de entorno temporal que restaura al terminar, incluso si falla.
La clave debug local debe existir; el flujo de pruebas anterior ya la utiliza.

Para una clave externa en otra ubicación, usa `-KeystorePath` y, si corresponde, `-KeyAlias`.
También se admiten `CARGO_APK_RELEASE_KEYSTORE` y `CARGO_APK_RELEASE_KEYSTORE_PASSWORD`
inyectadas por un gestor de secretos para automatización; no las guardes en archivos versionados
ni escribas contraseñas literales en comandos. La contraseña de la clave privada debe coincidir
con la del almacén, como ocurre con el PKCS12 generado por el script.

La primera instalación con esta clave **no puede actualizar la app con firma debug**.
Desinstalarla elimina sus datos privados y trabajos temporales; conserva primero lo necesario.
Los archivos ya publicados en Descargas están fuera del almacenamiento privado. El build
no desinstala ni instala nada automáticamente. Después de esa migración, conserva siempre
la misma clave y aumenta `package.version` en `Cargo.toml` para cada actualización
(`cargo-apk` deriva de ella el código de versión Android).

## Organización

```text
yt_descarga/
├── assets/
│   └── android-runtime/       # zipapp, biblioteca Python y manifiesto
├── jniLibs/arm64-v8a/         # bibliotecas y launcher Android nativos
├── scripts/                   # generación del manifiesto Android
├── src/
│   ├── android_yt_dlp.rs      # validación e instalación del runtime privado
│   ├── app.rs                 # interfaz egui
│   ├── downloader.rs          # ejecución Android y canal de logs
│   ├── lib.rs                 # punto de entrada android_main
│   ├── platform.rs            # integración JNI con Android
│   └── runner/                # resolución del runtime y comandos yt-dlp
├── AndroidManifest.xml
└── Cargo.toml
```

`src/main.rs`, `src/runner/instalador.rs` y los archivos web que aún existen en el repositorio no forman parte del build Android actual. Se conservan como material legado para una posible etapa futura de escritorio/web.

## Documentación

- [`DOCUMENTACION.md`](./DOCUMENTACION.md): flujo y arquitectura Android.
- [`assets/README.md`](./assets/README.md): formatos, hashes y requisitos del runtime.
- [`src/README.md`](./src/README.md): descripción de los módulos Rust.
