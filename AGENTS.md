# Directrices del proyecto

## Alcance y arquitectura

La versión activa es Android nativo para `arm64-v8a`. La librería Rust se inicia en `android_main`, presenta la UI con egui y delega las descargas a hilos secundarios. El build actual no debe incluir ejecutables de escritorio, WASM ni el servidor web.

El runtime de yt-dlp depende de CPython PIE Bionic, el zipapp y el manifiesto versionado descritos en [`assets/README.md`](assets/README.md). No renombrar una biblioteca `.so` como si fuera un ejecutable.

## Reglas de desarrollo

1. Mantener bajo el uso de memoria y evitar clones innecesarios.
2. Ejecutar procesos externos fuera del hilo de UI y comunicar su salida mediante canales `mpsc`.
3. Validar cambios Android con el target `aarch64-linux-android`; la compilación del APK no sustituye las pruebas en un dispositivo ARM64.
4. Mantener la documentación y los errores explícitos cuando falten artefactos del runtime.
