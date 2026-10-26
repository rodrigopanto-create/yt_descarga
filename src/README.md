# Módulos Rust de Android

El crate se compila como librería Android (`cdylib`) para `aarch64-linux-android`. El ejecutable de escritorio está desactivado en Cargo; esta carpeta documenta solo la ruta activa de Android.

## Módulos

- `lib.rs`: entrada `android_main`, inicialización de logs y arranque de `eframe`.
- `app.rs`: interfaz, acciones y actualización de logs.
- `platform.rs`: acceso al portapapeles mediante JNI.
- `downloader.rs`: ejecución de yt-dlp fuera del hilo de UI.
- `android_yt_dlp.rs`: preparación y validación del runtime CPython/yt-dlp en Android.
- `runner/finder.rs`: obtiene las rutas instaladas por el runtime Android.
- `runner/process.rs`: ejecuta procesos de validación con límite de tiempo y salida acotada.
- `runner/ytdlp.rs`: opciones del comando yt-dlp.
- `runner/ffmpeg.rs`: añade ffmpeg cuando el runtime lo valida.

El contrato de los binarios y assets está documentado en [`../assets/README.md`](../assets/README.md).
