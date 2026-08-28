# YouTube Downloader Portable (yt-dlp + ffmpeg)

Una interfaz gráfica (GUI) liviana, rápida y multiplataforma desarrollada en **Rust** utilizando **eframe / egui**. Permite descargar audio en alta calidad (MP3) y video (MP4) de YouTube ejecutando `yt-dlp` y `ffmpeg` en segundo plano mediante hilos no bloqueantes.

---

## 🚀 Características Principales

* **Estrategia de Búsqueda de Binarios en 3 Niveles:**
  1. **PATH Global:** Detecta y usa automáticamente `yt-dlp` o `ffmpeg` si están instalados en el sistema (vía `pip`, gestores de paquetes, etc.).
  2. **Binarios Embebidos / Locales:** Soporta inclusión nativa de binarios compilados en la aplicación (*Full Build*) o librerías nativas `.so` (Android).
  3. **Fallback Automático (GitHub):** Si la app no detecta binarios locales ni en PATH, los descarga automáticamente en su última versión desde el repositorio oficial de GitHub a un directorio ejecutable privado.
* **Dual Build (Lite vs Full):** 
  * **Versión Lite (Default):** Peso ultra liviano (~5–10 MB). Libre de binarios pesados al compilar; descarga o detecta dependencias al vuelo.
  * **Versión Full:** Peso completo. Embebe `yt-dlp` y `ffmpeg` dentro del ejecutable/APK para funcionamiento 100% offline.
* **Auto-actualización Activa:** Ejecuta `yt-dlp -U` antes de cada descarga para garantizar compatibilidad con los cambios constantes de YouTube.
* **Soporte Multiplataforma:** Compila de forma nativa para **Windows** (escritorio) y **Android** (APK nativo con soporte JNI para el portapapeles).
* **Consola en Vivo:** Muestra el progreso de descarga y errores en tiempo real dentro del panel log de la interfaz gráfica.
* **UI No Bloqueante:** Comunicación asíncrona mediante canales MPSC (`std::sync::mpsc`) para mantener la interfaz fluida y reactiva.

---

## 🛠️ Requisitos para Compilar

1. Tener instalado **Rust** (`cargo` y `rustc`).
2. *(Opcional)* Si vas a compilar la versión **Full**, coloca los binarios ejecutables correspondientes en la carpeta `bin/`:
   * `bin/yt-dlp.exe` (Windows) / `bin/yt-dlp` (Linux/Unix)
   * `bin/ffmpeg.exe` (Windows) / `bin/ffmpeg` (Linux/Unix)

---

## 📁 Estructura del Proyecto

```text
yt_descarga/
├── bin/                   # (Opcional) Binarios locales para compilación Full
├── src/
│   ├── app.rs             # Interfaz gráfica (egui) y ciclo de vida de la UI
│   ├── downloader.rs      # Gestor de hilo secundario, canales MPSC y streaming
│   ├── lib.rs             # Configuración de punto de entrada (Desktop / Android)
│   ├── main.rs            # Entrypoint ejecutable de escritorio
│   ├── platform.rs        # Funciones nativas de plataforma (ej. JNI en Android)
│   └── runner/
│       ├── ffmpeg.rs      # Construcción e inyección de comandos ffmpeg
│       ├── finder.rs      # Motor de búsqueda y descarga remota de binarios
│       ├── mod.rs         # Módulo expuesto
│       └── ytdlp.rs       # Construcción de comandos y argumentos de yt-dlp
├── Cargo.toml
└── README.md
