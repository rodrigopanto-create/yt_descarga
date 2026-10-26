# Documentación de yt_descarga

## Alcance

La aplicación activa se compila únicamente como librería Android para `arm64-v8a`. `Cargo.toml` desactiva la detección automática de ejecutables para que `src/main.rs` no se construya como aplicación de escritorio. El código PWA/WASM y el servidor web local quedan fuera del build actual.

## Arquitectura

- `src/lib.rs`: configura el logger, inicializa el `AssetManager` y arranca `eframe` desde `android_main`.
- `src/app.rs`: interfaz egui, estado de descarga, portapapeles Android y logs.
- `src/platform.rs`: lectura del portapapeles y publicación de descargas con MediaStore a través de JNI.
- `src/downloader.rs`: resuelve los binarios, ejecuta el proceso en un hilo secundario y publica los resultados.
- `src/runner/ytdlp.rs`: crea comandos para audio MP3 o video.
- `src/runner/finder.rs`: delega la preparación de herramientas al runtime Android.
- `src/android_yt_dlp.rs`: valida el manifiesto y hashes, instala el runtime versionado bajo el directorio privado de la app, genera un bundle PEM desde los certificados del sistema Android y configura el wrapper para usarlo al validar HTTPS.
- `assets/android-runtime/`: zipapp de yt-dlp y archivos de CPython enumerados en el manifiesto.
- `jniLibs/arm64-v8a/`: launcher PIE y bibliotecas compartidas nativas incluidas en el APK.
- `scripts/Generate-AndroidRuntimeManifest.ps1`: calcula los hashes y crea el manifiesto a partir de los artefactos preparados.
- `scripts/Build-FFmpegAndroid.ps1` y `android/build_ffmpeg.sh`: compilan FFmpeg/FFprobe ARM64 con LAME estático usando el NDK, Git Bash y herramientas host de Visual Studio.

El detalle de los artefactos compatibles y el proceso de preparación está en [`assets/README.md`](./assets/README.md).

## Flujo de descarga

1. El usuario pega o escribe una URL y elige MP3 o MP4.
2. La UI valida que el campo no esté vacío y ejecuta la tarea en un hilo secundario.
3. `finder.rs` prepara el runtime CPython/yt-dlp y valida FFmpeg y FFprobe juntos si están declarados. Una herramienta declarada pero corrupta o incompatible produce un error explícito. Si no se incluyen, MP3 se rechaza antes de descargar.
4. `ytdlp.rs` construye el comando y `/system/bin/sh` lanza el wrapper.
5. stdout y stderr se leen concurrentemente y se reenvían por `mpsc` a la interfaz.
6. El marcador `__FIN__` vuelve a habilitar los controles.
7. Si yt-dlp termina correctamente, los archivos de la carpeta temporal se copian a `Descargas/yt_descarga` usando MediaStore en Android 10 o posterior. En Android anterior a 10 se copian al directorio externo privado de la app, sin solicitar permisos de almacenamiento amplios.

Las descargas incompletas o que no se puedan publicar se conservan en el almacenamiento privado temporal de la app y se informa su ubicación en los logs.

## Compilación

Target configurado: `aarch64-linux-android`.

```powershell
rustup target add aarch64-linux-android
.\scripts\Build-Android.ps1
```

Se requiere Rust, Android NDK y `cargo-apk`. El script conserva los directorios estándar CPython que empiezan con `_` al empaquetar assets. Para una build release, crea una clave fuera del repositorio con `scripts/New-AndroidSigningKey.ps1` y ejecuta el script de build con `-Release`. El procedimiento, las copias de seguridad y la migración desde firma debug están en [Firma de distribución](./README.md#firma-de-distribución-apk-directo). La compilación del APK no reemplaza la validación funcional del runtime: probar la descarga en un dispositivo ARM64 es un paso aparte.

`Build-Android.ps1` prepara una lista explícita de bibliotecas en `target/android-runtime-libs`,
que es la ruta consumida por `cargo-apk`. Excluye MPV y sus dependencias incompletas del APK
de descargas. Para pruebas optimizadas permite `-Release -DebugSigning`, usando exclusivamente
el keystore debug local y sin modificar la configuración de firma de publicación.

Para distribución, la firma final se realiza con `apksigner` mediante una variable de
entorno temporal, sin poner la contraseña privada en los argumentos del proceso. El APK
intermedio de `cargo-apk` usa la clave debug; solo el APK final verificado en
`target/release/distribution/yt_descarga.apk` se debe compartir. Se rechazan claves dentro
del repositorio, certificados debug o vencidos y cualquier diferencia entre el certificado
de la clave seleccionada y el del APK final. El script no instala ni desinstala la app.
Se generó el APK con la clave definitiva y se verificaron independientemente las firmas
v1/v2/v3, el certificado `CN=yt_descarga` (RSA de 3072 bits), `debuggable=false` y la
conservación de los hashes de FFmpeg/FFprobe. Esta build no reemplazó la app debug instalada.

## Estado verificado

En un dispositivo ARM64 con Android 12 (API 31) se verificaron la descarga MP4 y la descarga
con conversión a MP3 desde la interfaz, publicadas mediante MediaStore (`is_pending=0`).
El MP3 de prueba contiene 634,58 segundos de audio estéreo a 44,1 kHz. Los ejecutables
FFmpeg/FFprobe 8.0.3 también convirtieron un fragmento a MP3 y unieron pistas H.264/AAC
separadas en MP4. Ambos dependen únicamente de `libc.so` y `libm.so`.

El APK de prueba del runtime usa compilación optimizada y firma debug local; el flujo de
distribución anterior utiliza una clave privada externa separada. El arranque de la build debug sin optimización
continúa afectado por la precondición de `ndk` documentada en el README.

El manifiesto `assets/android-runtime/manifest.json` enumera y verifica los archivos Python y
los hashes de FFmpeg/FFprobe. `libpython3.15.so` sigue siendo una biblioteca compartida
separada; no se usa como ejecutable.

## Código diferido

`src/main.rs`, `src/runner/instalador.rs`, `index.html`, `web/` y los archivos PWA permanecen en el repositorio, pero no son compilados ni forman parte del flujo Android. Se mantienen separados del destino actual para retomarlos cuando se inicie el trabajo de escritorio/web.
