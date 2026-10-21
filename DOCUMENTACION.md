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

- `finder.rs`: resolución y validación de `yt-dlp`/`ffmpeg`.
  - Escritorio: busca `yt-dlp` en `PATH`, en `%LOCALAPPDATA%/yt_descarga/bin` (o equivalente) y en `bin/` local. Si no hay candidato válido, usa descarga de respaldo (`descargar_fallback_github`) al directorio gestionado.
  - Android: prepara CPython PIE Bionic desde `nativeLibraryDir/libpython3.so`, verifica el manifiesto SHA-256 y extrae el zipapp/runtime a `files/python-runtime/<version>` mediante staging en `files/tmp`. Genera `files/bin/yt-dlp`, un wrapper ejecutado por `/system/bin/sh`. No trata `libytdlp.so` como CLI.
- `instalador.rs`: descarga e instalación segura desde GitHub Releases:
  1. Obtiene la etiqueta de `releases/latest`.
  2. Comprueba el SHA-256 contra `SHA2-256SUMS` de esa misma release.
  3. Descarga por streaming a un temporal en el directorio de destino, con límites de tamaño.
  4. Valida la cabecera PE/ELF/Mach-O y la arquitectura.
  5. Ejecuta `--version` con un timeout de 120 s.
  6. Instala con `rename` sobre el destino, sin borrar antes la versión anterior.
  7. Ante cualquier fallo, elimina el temporal y conserva el binario previo.
  - Limitación: no se verifica la firma GPG (`SHA2-256SUMS.sig`). Por eso el hash detecta corrupción, truncado y mezclas entre releases, pero no protege frente a una release comprometida en origen.
- `ytdlp.rs`: argumentos para `yt-dlp`
- `ffmpeg.rs`: inyección de la ruta de ffmpeg

### 4. Entrada de la app

- `src/lib.rs`: exports para la librería y puntos de entrada para Android y WebAssembly
- `src/main.rs`: ejecutable de escritorio y backend web local

## Requisitos

### Escritorio

- Rust / Cargo instalado
- `yt-dlp` disponible en `bin/`; si falta, se descarga y verifica automáticamente (Windows x86/x64/ARM64, macOS, Linux x86_64/aarch64)
- `ffmpeg` disponible en `bin/` si se desea extraer audio o convertir vídeo

### Android

- CPython PIE compilado con NDK para Bionic/AArch64 en `jniLibs/arm64-v8a/libpython3.so`
- zipapp y manifiesto con hashes en `assets/android-runtime/`; el formato completo se documenta en [`assets/README.md`](./assets/README.md)
- La primera instalación valida cada hash y la versión CPython/yt-dlp antes de activar el runtime versionado. Los archivos de `files/` no se ejecutan directamente: el wrapper usa `/system/bin/sh` para respetar W^X.
- ffmpeg es opcional; solo se usa si se declara su hash y pasa validación ELF, arquitectura y `-version`.
- Los artefactos actuales `assets/yt-dlp.bin` y `jniLibs/arm64-v8a/libytdlp.so` no son compatibles por defecto. Si falta el nuevo runtime, el resolvedor devuelve un error claro.

### WebAssembly

- se requiere `wasm-pack` para compilar el frontend
- el backend nativo sigue sirviendo la interfaz y la API local
- el servidor local escucha en el puerto `8082`
- la interfaz es instalable como PWA en localhost o HTTPS y cachea el shell web; las descargas necesitan conexión con el backend

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

- En escritorio, si falta `yt-dlp`, se descarga de GitHub Releases y solo se instala si pasa todas las verificaciones. `ffmpeg` debe colocarse a mano.
- En Android, los binarios deben ir empaquetados en `jniLibs`.
- Si `ffmpeg` no existe, la descarga puede seguir funcionando para ciertos flujos, pero no siempre será posible convertir o combinar streams.

## Siguientes mejoras sugeridas

- detección automática y descarga de dependencias faltantes
- mejor manejo de errores de red o de YouTube
- progreso visual más preciso
- validación de URLs antes de iniciar la descarga
- soporte más sólido para conversiones y rutas personalizadas
