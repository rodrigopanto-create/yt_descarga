# YouTube Downloader Portable

Aplicación multiplataforma escrita en Rust con interfaz gráfica basada en `eframe` / `egui`. Permite descargar audio y video desde YouTube usando `yt-dlp` y opcionalmente `ffmpeg`, con soporte para escritorio, Android y modo web local.

## ¿Qué hace?

- Descarga archivos en formato MP3 o MP4 desde una URL de YouTube.
- Ejecuta `yt-dlp` en segundo plano sin bloquear la interfaz.
- Guarda los archivos en la carpeta de descargas del sistema o en `/sdcard/Download` en Android.
- Soporta dos modos:
  - Nativo: ventana desktop o aplicación Android.
  - Web local: backend Actix sirve la interfaz y expone una API para descargar desde la red local.

## Estado actual del proyecto

La implementación actual está enfocada en un flujo de trabajo simple y robusto:

- El binario de `yt-dlp` se resuelve desde `bin/` en escritorio.
- En Android, se busca en el directorio nativo de librerías (`nativeLibraryDir`).
- `ffmpeg` se inyecta únicamente si existe junto con `yt-dlp`.
- La UI se actualiza con logs en tiempo real mediante canales `mpsc`.
- El servidor web escucha en `http://127.0.0.1:8080` y también responde en la red local.

## Requisitos

1. Instalar Rust y Cargo.
2. Opcional: `wasm-pack` si quieres compilar la versión web de prueba.
3. Preparar los binarios locales:
   - Windows: `bin/yt-dlp.exe` y `bin/ffmpeg.exe`
   - Linux/macOS: `bin/yt-dlp` y `bin/ffmpeg`
4. En Android, la app espera encontrar `libytdlp.so` y opcionalmente `libffmpeg.so` dentro del directorio de librerías nativas del paquete.

## Ejecución

### Modo nativo

```bash
cargo run
```

Esto abre la ventana principal de la app y arranca el servidor web local en segundo plano.

### Modo web / WASM

Primero compila el cliente web:

```bash
wasm-pack build --target web --out-dir web
```

Luego ejecuta la app nativa para servir la interfaz y la API:

```bash
cargo run
```

La interfaz web queda disponible normalmente en `http://127.0.0.1:8080`.

## Estructura del proyecto

```text
yt_descarga/
├── bin/                  # Binarios locales esperados para desktop
├── src/
│   ├── app.rs            # UI principal en egui
│   ├── downloader.rs     # Lógica de descarga para nativo y web
│   ├── lib.rs            # Puntos de entrada para Android y WASM
│   ├── main.rs           # Ejecutable desktop + API web local
│   ├── platform.rs       # Integración nativa para Android
│   └── runner/
│       ├── ffmpeg.rs     # Inyección de ffmpeg al comando
│       ├── finder.rs     # Resolución de rutas de yt-dlp/ffmpeg
│       ├── mod.rs        # Exportación del módulo runner
│       └── ytdlp.rs      # Construcción de comandos yt-dlp
├── Cargo.toml
├── README.md
├── src/README.md
├── DOCUMENTACION.md
└── web/                  # Salida de wasm-pack
```

## Consideraciones importantes

- El proyecto no realiza descarga automática de `yt-dlp` desde GitHub en esta versión.
- No hay actualización automática de `yt-dlp` con `-U` en cada descarga a través del código actual.
- Para un uso correcto, es recomendable dejar los ejecutables en `bin/` antes de compilar para escritorio.

## Siguientes mejoras sugeridas

- Añadir descarga automática de dependencias faltantes.
- Mejorar manejo de errores y mensajes por tipo de fallo.
- Añadir validación de URL antes de enviar la descarga.
- Mejorar el panel de logs y añadir barra de progreso real.
- Añadir un flujo de instalación más claro para binarios nativos.