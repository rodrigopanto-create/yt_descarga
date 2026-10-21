# YouTube Downloader Portable

Aplicación multiplataforma en Rust con interfaz `eframe` / `egui`, modos Lite/Full y ejecución
de `yt-dlp` y `ffmpeg` en hilos secundarios. La interfaz muestra los logs mediante canales MPSC.
El backend Actix permite usar la interfaz WASM desde el navegador.

## GitHub Pages / PWA

La interfaz se publica en https://rodrigopanto-create.github.io/yt_descarga/ mediante
[GitHub Actions](.github/workflows/pages.yml).

**GitHub Pages solo aloja la interfaz estática. No ejecuta Rust, yt-dlp ni ffmpeg.**
La publicación es una demostración instalable; las descargas se bloquean con un mensaje
explícito. Para descargar, usa la aplicación nativa o el servidor local.

El workflow compila el cliente WASM desde el código Rust, prepara un directorio que solo
contiene recursos públicos y lo despliega con `actions/deploy-pages`. No publica el repositorio
completo, binarios nativos, assets Android ni credenciales.

En **Settings > Pages > Build and deployment**, seleccionar **GitHub Actions** como origen.
Cada push a `main` despliega automáticamente; también puede iniciarse desde **Actions >
Deploy PWA to GitHub Pages > Run workflow**.

El manifiesto, iconos y service worker usan rutas relativas para funcionar tanto en
`/yt_descarga/` como en la raíz del servidor local. Después de la primera carga correcta, los
recursos de interfaz quedan disponibles sin conexión. Las peticiones de API no se cachean ni
se reproducen. Para actualizar el contenido cacheado al modificar los recursos web, incrementar
la versión de caché en `sw.js`.

## Ejecución local

Instala Rust y, para generar la interfaz web, `wasm-pack`:

```sh
wasm-pack build --target web --out-dir web
cargo run
```

Abre http://127.0.0.1:8082. El backend debe permanecer activo para descargar; los archivos se
guardan en el equipo donde corre el servidor. El servidor también escucha en la red local.
La instalación PWA requiere localhost o HTTPS; una dirección LAN HTTP no ofrece el mismo soporte.

## Modos y dependencias

- **Lite**: compilación predeterminada; localiza los binarios del sistema o del directorio local.
- **Full**: permite incorporar binarios preparados en `bin/` mediante `full-binaries`.
- Prepara `yt-dlp` y `ffmpeg` compatibles con el sistema de destino. En Windows los archivos
  usan extensión `.exe`.
- Android necesita su propia integración y binarios compatibles con Bionic; los ejecutables
  Linux convencionales no deben asumirse compatibles.
  El código y los artefactos disponibles, junto con las limitaciones de empaquetado y ejecución
  pendientes, se describen en [assets/README.md](assets/README.md). Incluir los binarios no
  significa que el runtime Android ya esté operativo; falta validar la integración en dispositivo.

## Estructura

- `src/app.rs`: interfaz egui.
- `src/downloader.rs`: descarga nativa y llamadas al backend desde WASM.
- `src/main.rs`: ejecutable de escritorio y servidor Actix.
- `src/lib.rs`: entradas de Android y WASM.
- `src/runner/`: resolución de herramientas y construcción de comandos.
- `index.html`, `manifest.webmanifest`, `sw.js`, `icons/`: shell PWA.
- `web/`: salida de compilación de wasm-pack.

Consulta también [DOCUMENTACION.md](DOCUMENTACION.md) y [src/README.md](src/README.md).
