# YouTube Downloader Portable (yt-dlp + ffmpeg)

Una interfaz gráfica (GUI) liviana, rápida y 100% portable desarrollada en **Rust** usando **eframe / egui**. Permite descargar audios en alta calidad (MP3 320kbps) y videos (MP4) de YouTube invocando `yt-dlp` y `ffmpeg` en segundo plano.

---

## Caracteristicas Principales

* **100% Portable:** Los binarios de `yt-dlp.exe` y `ffmpeg.exe` están embebidos en el propio ejecutable. No requiere instalación de dependencias externas en el sistema.
* **Autoactualizable:** Ejecuta automáticamente `yt-dlp -U` antes de cada descarga para garantizar compatibilidad con los cambios recientes de YouTube.
* **Deteccion Automatica:** Guarda los archivos descargados directamente en la carpeta de *Música* del usuario activo (`C:\Users\<Usuario>\Music`).
* **Consola en Vivo:** Muestra el progreso de descarga y salida de terminal en tiempo real dentro de la misma interfaz gráfica.
* **No Bloqueante:** Procesa las descargas en un hilo (*thread*) secundario para mantener la interfaz fluida en todo momento.

---

## Requisitos para Compilar

Si deseas clonar y compilar este proyecto desde el código fuente, necesitas:

1. Tener instalado **Rust** (`cargo` y `rustc`).
2. Descargar los ejecutables para Windows y colocarlos en la carpeta `bin/` del proyecto:
   * `bin/yt-dlp.exe`
   * `bin/ffmpeg.exe`

### Estructura de Directorios Requerida

```text
yt_descarga/
├── bin/
│   ├── ffmpeg.exe
│   └── yt-dlp.exe
├── src/
│   └── main.rs
├── Cargo.toml
└── README.md