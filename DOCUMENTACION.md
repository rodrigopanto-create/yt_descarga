# Documentación de yt_descarga

## Descripción general

`yt_descarga` es una aplicación híbrida desarrollada en Rust para descargar contenido de YouTube en formatos de audio y video. Está pensada para funcionar en distintos entornos:

- escritorio (Windows, Linux, macOS)
- Android
- web local con servidor Actix y cliente WASM

La lógica de descarga se apoya en `yt-dlp` y, cuando está disponible, `ffmpeg` para tareas de extracción y conversión.

## Arquitectura

### 1. Interfaz gráfica

La UI principal vive en `src/app.rs` y se basa en `eframe` / `egui`. Permite:

- introducir una URL
- iniciar descarga como MP3 o MP4
- observar logs en tiempo real
- detectar el portapapeles en Android

### 2. Módulo de descarga

`src/downloader.rs` centraliza la ejecución de ejecuciones externas. Maneja dos caminos:

- nativo: utiliza `std::process::Command`
- web/WASM: envía la petición al backend Actix

### 3. Runner de binarios

`src/runner/` encapsula la construcción de los comandos y la resolución de rutas:

- `finder.rs`: ubicación de `yt-dlp` y `ffmpeg`
- `ytdlp.rs`: argumentos para `yt-dlp`
- `ffmpeg.rs`: inyección de la ruta de ffmpeg

### 4. Entrada de la app

- `src/lib.rs`: exports para la librería y puntos de entrada para Android y WebAssembly
- `src/main.rs`: ejecutable de escritorio y backend web local

## Requisitos

### Escritorio

- Rust / Cargo instalado
- `yt-dlp` disponible en `bin/`
- `ffmpeg` disponible en `bin/` si se desea extraer audio o convertir vídeo

### Android

- la librería nativa se espera en `nativeLibraryDir`
- se usan archivos `libytdlp.so` y opcionalmente `libffmpeg.so`

### WebAssembly

- se requiere `wasm-pack` para compilar el frontend
- el backend nativo sigue sirviendo la interfaz y la API local

## Flujo de descarga

1. El usuario introduce la URL.
2. La app valida que no esté vacía.
3. Se resuelve el ejecutable apropiado para la plataforma.
4. Se arma el comando de `yt-dlp` con formato MP3 o MP4.
5. El proceso se ejecuta en un hilo secundario.
6. La salida se reenvía a la UI.
7. La aplicación detecta el fin de la descarga y vuelve a habilitar los botones.

## Directorio esperado

```text
yt_descarga/
├── bin/
│   ├── yt-dlp
│   ├── yt-dlp.exe
│   ├── ffmpeg
│   └── ffmpeg.exe
├── src/
├── Cargo.toml
├── README.md
├── DOCUMENTACION.md
├── src/README.md
└── web/
```

## Consideraciones de uso

- El proyecto actual no descarga automáticamente `yt-dlp` desde GitHub.
- Los binarios deben existir en la ruta correcta antes de compilar o ejecutar la versión nativa.
- Si `ffmpeg` no existe, la descarga puede seguir funcionando para ciertos flujos, pero no siempre será posible convertir o combinar streams.

## Siguientes mejoras sugeridas

- detección automática y descarga de dependencias faltantes
- mejor manejo de errores de red o de YouTube
- progreso visual más preciso
- validación de URLs antes de iniciar la descarga
- soporte más sólido para conversiones y rutas personalizadas
