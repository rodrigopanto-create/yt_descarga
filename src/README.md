# Documentación interna del proyecto

Esta carpeta contiene la lógica principal del cliente y de la descarga. El flujo principal se compone de una interfaz en `egui`, un conjunto de comandos para `yt-dlp` y una capa de resolución de binarios que adapta el comportamiento según la plataforma.

## Módulos principales

### `app.rs`

Define la UI del usuario y gestiona el estado de la aplicación:

- entrada de URL
- botón para descargar MP3 o MP4
- estado de descarga activo/inactivo
- consola de logs en pantalla
- acceso al portapapeles en Android

El caso de WASM usa un canal `mpsc` para devolver mensajes desde el backend al frontend, mientras que el caso nativo ejecuta el proceso directamente en un hilo.

### `downloader.rs`

Es el punto central de la descarga:

- prepara el comando según el tipo de archivo
- ejecuta el proceso en un hilo separado
- recolecta stdout/stderr y los reenvía a la UI
- envía el código `__FIN__` para indicar el final del proceso

La implementación difiere entre:

- `not(target_arch = "wasm32")`: proceso nativo usando `std::process::Command`
- `target_arch = "wasm32"`: solicitud HTTP a la API del backend local

### `runner/`

Contiene la lógica de construcción de comandos y resolución de ejecutables.

#### `finder.rs`

Resuelve la ruta de `yt-dlp` y, si existe, de `ffmpeg`:

- escritorio: `bin/yt-dlp` o `bin/yt-dlp.exe`
- Android: `nativeLibraryDir/libytdlp.so`
- WASM: usa un valor ficticio porque no se ejecuta dentro del navegador

#### `ytdlp.rs`

Construye los comandos de `yt-dlp` con opciones de:

- extracción de audio `-x --audio-format mp3`
- no playlist
- nombres restringidos
- cliente Android del extractor de YouTube
- salida en la carpeta de descargas

#### `ffmpeg.rs`

Añade la ubicación de `ffmpeg` a la línea de comandos cuando se dispone del ejecutable.

### `lib.rs`

Define las entradas de la librería:

- `YtApp` visible para el ejecutable principal
- código específico para Android (`android_main`)
- punto de arranque de WASM para `eframe`

### `main.rs`

Es el ejecutable de escritorio y también el backend local:

- inicia el servidor Actix en el puerto 8082
- expone la ruta `/api/status`
- expone la ruta `/api/download`
- sirve la interfaz estática desde `./web` o `index.html`

## Flujo de descarga

1. El usuario introduce una URL y pulsa MP3 o MP4.
2. La UI llama a `ejecutar_descarga` desde `downloader.rs`.
3. El módulo `runner` resuelve la ruta de `yt-dlp` y `ffmpeg`.
4. Se construye el comando con `YtDlpCommand`.
5. El proceso se lanza en hilo separado.
6. Los logs se transmiten al cliente mediante `Sender<String>`.
7. Cuando termina, la UI reanuda el estado de la aplicación.

## Recomendaciones de desarrollo

- Mantener la salida de logs útil y legible para depuración.
- No asumir que `ffmpeg` siempre existe; manejar la ausencia de forma controlada.
- Probar la descarga en escritorio y Android por separado.
- Evitar cambiar la estructura de `runner` sin revisar los comandos construidos.
- Mantener la documentación sincronizada con la implementación real del proyecto.

## Ejemplo de comando generado

```bash
bin/yt-dlp -x --audio-format mp3 --no-playlist --no-warnings --restrict-filenames --extractor-args youtube:player_client=android -f ba/b -o "C:/Users/Usuario/Downloads/%(title)s.%(ext)s" "https://youtube.com/watch?v=..."
```