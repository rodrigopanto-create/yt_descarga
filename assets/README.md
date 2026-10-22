# Runtime Python para Android (arm64-v8a)

Android no ejecuta `assets/yt-dlp.bin` ni asume que `libytdlp.so` es un CLI. El runtime soportado
por el código es CPython compilado para Bionic/AArch64 más el zipapp oficial de yt-dlp.

## Estado de los archivos encontrados en el repositorio

- `assets/android-runtime/yt-dlp.pyz` es el zipapp de yt-dlp **2026.08.19**; su SHA-256
  (`1fa6733c...ed8d4d6`) coincide con el asset `yt-dlp` de esa release. Ya está en la ruta que
  usa el instalador Android.
- `jniLibs/arm64-v8a/libpython3.15.so` es una biblioteca compartida ELF AArch64 (`ET_DYN`) con
  `SONAME=libpython3.15.so` y entrypoint `0`. Su ubicación JNI es correcta, pero **no es el
  ejecutable PIE** `libpython3.so` que actualmente requiere el runtime. No cambiarle el nombre:
  Android no puede lanzarla como proceso. El bootstrap PIE separado, enlazado con esta
  biblioteca, se empaqueta como `libpython3.so`.
- `assets/android-runtime/python/lib/python3.15/` contiene la biblioteca estándar expandida y
  módulos nativos CPython `3.15` para AArch64 Android. Esto satisface el layout de archivos que
  consume el instalador del runtime. Las dependencias de `_ssl`, `_hashlib` y `_sqlite3`
  (`libssl_python.so`, `libcrypto_python.so` y `libsqlite3_python.so`) también están presentes en
  `jniLibs/arm64-v8a/`.
- `bin/yt-dlp` es el ejecutable autónomo Linux AArch64 de yt-dlp. Tiene cargador
  `/lib/ld-linux-aarch64.so.1` (glibc); sirve para Linux AArch64 compatible, **no para Android**.
- `bin/ffmpeg.exe` es el ejecutable Windows x64. `jniLibs/arm64-v8a/libffmpeg.so` era una copia
  con extensión engañosa de ese PE y se retiró de `jniLibs`. Ahora esa ruta contiene un CLI
  PIE Bionic compilado desde FFmpeg 9.0.2, junto con `libffprobe.so`.
- `jniLibs/arm64-v8a/libytdlp.so` era el mismo ejecutable Linux/glibc AArch64, no una biblioteca
  Android ni un CLI Bionic; se quitó de `jniLibs`.

El archivo `python3_pie` en la raíz es un launcher ELF PIE AArch64 con entrypoint no nulo,
cargador `/system/bin/linker64` y dependencia de `libpython3.15.so`; su fuente está en
`android/launcher.c`. Ya está copiado como `jniLibs/arm64-v8a/libpython3.so`, y
`assets/android-runtime/manifest.json` enumera los 2711 archivos de CPython con sus hashes.
En un dispositivo ARM64 con Android 12 se verificaron el arranque, la extracción del runtime,
los imports esenciales, Python `3.15.0` y yt-dlp `2026.08.19`. La prueba inicial con
`https://example.invalid/` terminó con el error DNS esperado; no era una descarga real.

Las bibliotecas compartidas antiguas de MPV/FFmpeg no constituyen una distribución completa: faltan
dependencias como `libavdevice.so`, `libavfilter.so`, `libavutil.so`, `libswresample.so`,
`libswscale.so` y `libc++_shared.so`. No se ha validado su ejecución ni la feature `mpv`.
Los nuevos CLI ffmpeg/ffprobe no dependen de esas bibliotecas: enlazan sus codecs estáticamente.
La firma de release se debe configurar fuera del repositorio, sin guardar contraseñas en
`Cargo.toml`.

## APK de prueba sin MPV

Se requieren el SDK Android, NDK, `cargo-apk` y un JDK (`JAVA_HOME`, o el JBR de Android Studio).
Los artefactos CPython proporcionados requieren como mínimo Android API 24.

```powershell
.\scripts\Build-AndroidDebug.ps1
# Con conversion MP3 y mezcla de pistas:
.\scripts\Build-AndroidDebug.ps1 -IncludeFfmpeg
```

El script prepara únicamente las cinco bibliotecas Python requeridas en
`target/android-runtime-libs/arm64-v8a/`, que es el `runtime_libs` configurado para cargo-apk.
No incluye MPV/FFmpeg incompletos. Regenera el manifiesto, conserva los directorios Python
que empiezan por `_` (AAPT los omite por defecto), firma con la clave de depuración local
y comprueba SHA-256 de todos los assets declarados y las cinco bibliotecas dentro del APK.
Con `-IncludeFfmpeg` prepara siete binarios y comprueba también las dos licencias empaquetadas
(2722 comprobaciones para el runtime actual). El staging rechaza bibliotecas ajenas al modo
elegido: tras compilar con ffmpeg, usar el mismo flag para reconstruir ese APK.
El APK se encuentra en `target/debug/apk/yt_descarga_lib.apk` (o bajo `CARGO_TARGET_DIR`).
No se instala ni se desinstala automáticamente ninguna app.

El manifiesto tiene un límite de 2 MiB y 4096 archivos. Los módulos vacíos de CPython se
aceptan únicamente cuando el manifiesto declara el SHA-256 del contenido vacío. Antes de
activar una nueva carpeta de runtime, se validan Python, los imports nativos esenciales
(`ssl`, `hashlib`, `sqlite3`, `ctypes`, `zlib`) y la versión de yt-dlp con timeout.

### Teclado y dependencias locales

`vendor/android-activity` conserva la licencia original y corrige el estado guardado nulo
del primer arranque, además de usar el decor de la ventana para mostrar/ocultar el teclado
(adaptación de https://github.com/rust-mobile/android-activity/pull/178).
`vendor/winit` conserva la versión 0.29.15 y su licencia: conecta las peticiones IME y el texto
de las teclas con Android. Se verificaron apertura del teclado, escritura y retroceso en el
dispositivo. La composición IME compleja no está implementada por NativeActivity 0.5.

### Red y Descargas públicas

El APK declara `android.permission.INTERNET`. En Android 10 o posterior, yt-dlp escribe en
un staging privado por intento; solo al terminar correctamente se copian los archivos a
MediaStore Downloads con `RELATIVE_PATH=Download/yt_descarga` e `IS_PENDING=1`. La entrada se
publica con `IS_PENDING=0` después de copiar, sincronizar y cerrar el descriptor. La UI solo
anuncia éxito cuando MediaStore confirma la publicación y muestra la URI `content://`.
No se solicitan permisos generales de almacenamiento ni se escribe directamente en
`/sdcard/Download`.

El OpenSSL de CPython no encuentra por defecto las CA de Android. En cada preparación se
genera atómicamente `files/bin/android-ca.pem` a partir del almacén de solo lectura del sistema:
`/apex/com.android.conscrypt/cacerts` si existe, o `/system/etc/security/cacerts`.
El wrapper configura `SSL_CERT_FILE` y `--compat-options no-certifi` para usar estas CA con
verificación de certificado y hostname activada. No se usa `--no-check-certificates`.
Se incluyen solo CA del sistema, no las añadidas por el usuario. Cada certificado tiene un
límite de 64 KiB y el bundle de 8 MiB; un fallo conserva el bundle anterior y bloquea esa
preparación en lugar de permitir HTTPS sin verificar.

Si falla la copia o publicación, se intenta eliminar la entrada pendiente y se conserva el
staging con su ruta para recuperación. Una interrupción abrupta puede dejar staging o entradas
pendientes; no hay recuperación automática de procesos terminados. Si una operación produce
varios archivos, cada publicación es independiente, no una transacción del conjunto.
Android 7-9 puede arrancar el runtime, pero guardar en Descargas por esta vía devuelve un error
explícito: falta implementar un selector SAF o el flujo de permisos de almacenamiento legado.
Sin un ffmpeg Android validado no se garantiza conversión a MP3 ni mezcla de pistas separadas.

## FFmpeg Android: origen, integridad y pruebas

La compilación está en `.github/workflows/android-ffmpeg.yml` y
`scripts/build-android-ffmpeg.sh`. Usa Ubuntu 24.04 y NDK r27c (27.2.12479018), ARM64/API 24:

- FFmpeg 9.0.2: tarball oficial y firma PGP verificada contra la huella fijada
  `FCF986EA15E6E293A5644F10B4322F04D67658D8`.
- LAME 3.100: tarball de SourceForge, SHA-256 fijado
  `ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e`,
  registrado independientemente en MSYS2 `mingw-w64-lame/PKGBUILD`. No se afirma que este hash
  equivalga a una firma del autor de LAME.
- GPL y nonfree desactivados; codecs estáticos, dependencias dinámicas solo del sistema Android.
  No incluye MPV ni habilita red en FFmpeg: yt-dlp descarga y FFmpeg procesa archivos locales.
- Build validado: https://github.com/rodrigopanto-create/yt_descarga/actions/runs/39839133795
  (commit de receta `70d3e636c51b7684142ad57a102d26f466983eea`).
- El ZIP `android/ffmpeg-distribution.zip` conserva el artefacto completo, fuentes originales,
  firma, claves, configuración, licencias, bibliotecas estáticas y objetos para reenlazado LGPL.
  Su SHA-256 es `a3c2146174a60e1f09a2e2b96fb3f8084edb1f9fc8cb29d80113de0eb44eadd3`;
  se comprobó contra el digest de Actions y luego contra todos los `SHA256SUMS` internos.
  El ZIP se conserva en Git para no depender de la caducidad de 30 días del artefacto de Actions.
  Al redistribuir el APK, mantener disponible este material correspondiente y sus licencias.
- Las licencias se empaquetan en `assets/android-runtime/licenses/`.

El manifiesto declara `ffmpeg_sha256`, `ffprobe_sha256` y `ffmpeg_version`. El resolvedor verifica
ambos hashes, formato PIE AArch64 con linker Android, permisos y versiones con timeout antes de
ofrecer la ruta a yt-dlp. Un ffprobe ausente o inválido deshabilita el par, no el runtime Python.
yt-dlp recibe la ruta `libffmpeg.so` y descubre su hermano `libffprobe.so`.

Pruebas en Android 12 ARM64:

- Conversión real desde la UI: tráiler Sintel a `Download/yt_descarga/trailer.mp3`,
  1 457 161 bytes (primera compilación). La repetición web con el APK final recibió HTTP 403
  de Cloudflare; no se desactivó TLS ni se intentó eludir esa restricción.
- El APK final pasó la prueba offline `scripts/test_android_ffmpeg.py` con el MP4 Sintel
  previamente descargado: el postprocesador FFmpegExtractAudio de yt-dlp invocó LAME,
  generó MP3 de 51.925333 s y FFmpeg lo decodificó completo.
- La misma prueba separó vídeo/audio y ejecutó el merger real de yt-dlp mediante
  `YoutubeDL.process_ie_result`: MP4 resultante con H.264 de 52.208333 s y AAC de 51.946667 s.
  Ambos streams deben durar más de 50 s; los archivos temporales se eliminan al finalizar.

Para repetir la prueba offline, copiar el MP4 Sintel y el script a ubicaciones legibles por el
UID de la app, configurar `PYTHONHOME` al runtime instalado, `LD_LIBRARY_PATH` a `nativeLibraryDir`
y ejecutar el Python empaquetado:

```text
libpython3.so -S test_android_ffmpeg.py <nativeLibraryDir> <MP4-Sintel-privado>
```

La prueba habilita URLs `file://` solo en su instancia local de YoutubeDL, no en la aplicación.
No demuestra compatibilidad con todos los codecs, HLS, otros dispositivos o todas las variantes
de YouTube. Este build sirve para MP3 y remux de los contenedores explícitos de la receta,
no para transcodificación arbitraria de vídeo.

#### Prueba real en Android 12 / ARM64

Se descargó desde la interfaz el tráiler de Sintel servido por W3C:
`https://media.w3.org/2010/05/sintel/trailer.mp4`.
Licencia: [CC BY 3.0, Blender Foundation](https://durian.blender.org/sharing/).
Atribución: Blender Foundation | https://www.sintel.org/.

- Archivo público: `Download/yt_descarga/trailer.mp4`, 4 372 373 bytes.
- SHA-256 observado: `b670602fa00934ca27c4351bb0efe7ea7a07fae57284e44226025eeed7c51254`.
  Es una huella de la prueba, no un checksum autenticado publicado por el proveedor.
- MediaStore: `is_pending=0`; URI de la primera prueba
  `content://media/external/downloads/1000000176` (el ID varía por dispositivo).
- Decodificación completa de vídeo y audio con `ffmpeg -v error -i <archivo> -f null -`:
  código de salida 0, sin errores.
- Un intento posterior contra `https://example.invalid/` no modificó el archivo publicado
  ni creó otra entrada pública.
- Repetir la descarga creó `trailer (1).mp4` con otra URI y `is_pending=0`, sin sobrescribir
  el primer archivo. El staging de los intentos correctos se eliminó.

Estas pruebas verifican HTTPS, el extractor genérico, descarga y publicación; no prueban
el extractor específico de YouTube, MP3, mezcla con ffmpeg ni otras versiones de Android.
Las pruebas Rust son offline; la prueba real en el teléfono sí requiere Internet.

## Artefactos que debe proporcionar el paquete de release

1. `jniLibs/arm64-v8a/libpython3.so`: **ejecutable bootstrap PIE**, no solo `libpython3.15.so`.
   Debe ser ELF64 little-endian, AArch64 (`EM_AARCH64`), `ET_DYN` con entrypoint no nulo,
   segmento ejecutable y `PT_INTERP=/system/bin/linker64`. Debe responder a `--version`.
   Empaquetarlo como librería nativa permite que Android lo extraiga en `nativeLibraryDir`,
   lugar ejecutable por el sistema; no copiar ejecutables a `files/`.
2. `assets/android-runtime/yt-dlp.pyz`: zipapp de la release de yt-dlp (ya instalado en esta ruta).
3. Los archivos de Python declarados en `assets/android-runtime/manifest.json`, bajo
   `assets/android-runtime/python/`. La biblioteca estándar puede estar expandida en
   `lib/python3.<minor>/` e incluir `os.py`, `encodings`, `lib-dynload` y todos los módulos
   dinámicos requeridos, en las rutas indicadas por el manifiesto.
4. Las bibliotecas nativas requeridas por los módulos (`libcrypto_python.so`,
   `libssl_python.so` y `libsqlite3_python.so` para los artefactos actuales), ya presentes en
   `jniLibs/arm64-v8a/`, junto con cualquier otra dependencia de `DT_NEEDED`.
5. Opcionalmente el par `jniLibs/arm64-v8a/libffmpeg.so` y `libffprobe.so`: ejecutables PIE Bionic,
   no bibliotecas compartidas ordinarias. Requiere ambos hashes y `ffmpeg_version` en el
   manifiesto; ambos pasan comprobación ELF, permisos y `-version`.

No se usan los artefactos actuales `assets/yt-dlp.bin` ni `jniLibs/arm64-v8a/libytdlp.so` como
runtime Python: el primero es un ejecutable Linux/glibc y el segundo no demuestra compatibilidad
con Bionic ni que sea un ejecutable CLI.

## Manifiesto

Crear `assets/android-runtime/manifest.json` con este esquema (todos los hashes deben ser SHA-256
hexadecimal de 64 caracteres calculados sobre los bytes exactos empaquetados):

```json
{
  "schema_version": 1,
  "runtime_version": "cpython-3.15.0-android-arm64-v1",
  "python_version": "3.15.0",
  "python_executable_sha256": "<SHA256 de jniLibs/arm64-v8a/libpython3.so>",
  "ytdlp_version": "2026.08.19",
  "ytdlp_sha256": "<SHA256 de android-runtime/yt-dlp.pyz>",
  "python_files": [
    {
      "asset": "android-runtime/python/lib/python3.15/os.py",
      "install_path": "lib/python3.15/os.py",
      "sha256": "<SHA256 del archivo>"
    }
  ],
  "ffmpeg_sha256": null,
  "ffprobe_sha256": null,
  "ffmpeg_version": null
}
```

`python_files` debe enumerar **todos** los archivos requeridos por la distribución CPython
expandida, incluidos módulos de `lib-dynload`; el ejemplo no es por sí solo un runtime ejecutable. El
intérprete debe aceptar `PYTHONHOME=<directorio privado>/python`, ejecutar `-S <zipapp> --version`
y mostrar exactamente las versiones declaradas. Al cambiar cualquier artefacto, incrementar
`runtime_version`; el programa conserva las versiones previas y no sobrescribe una versión
corrupta con el mismo identificador.

Con los artefactos ya preparados, genera el JSON y sus hashes desde la raíz del repositorio:

```powershell
.\scripts\Generate-AndroidRuntimeManifest.ps1 `
  -RuntimeVersion cpython-3.15.0-android-arm64-v1 `
  -PythonVersion 3.15.0 `
  -YtDlpVersion 2026.08.19
```

El script requiere `jniLibs/arm64-v8a/libpython3.so`, el árbol expandido
`assets/android-runtime/python/lib/python3.<minor>/` (con `os.py`, `encodings` y módulos `.so` en
`lib-dynload`) y `assets/android-runtime/yt-dlp.pyz`. Para incluir ffmpeg,
añadir `-IncludeFfmpeg -FfmpegVersion 9.0.2` después de empaquetar y revisar ambos CLI.

El APK firmado es la raíz de confianza local del manifiesto y los assets. Los SHA-256 detectan
cambios accidentales y discrepancias de empaquetado, pero no protegen si el APK/manifiesto se
construye desde artefactos comprometidos. Para la procedencia upstream del zipapp, comparar
además su hash con `SHA2-256SUMS` de la misma release antes de incorporarlo.

## Instalación y ejecución

- `files/python-runtime/<runtime_version>/`: runtime versionado y zipapp, extraídos en streaming,
  con `create_new`, hash y `sync_all`.
- `files/tmp/`: staging temporal; se renombra al directorio versionado solo después de validar
  todos los assets. Los staging incompletos se limpian en el siguiente inicio.
- `files/bin/yt-dlp`: wrapper shell generado por la aplicación. Se invoca como
  `/system/bin/sh <wrapper> <argumentos>`; el wrapper configura `PYTHONHOME` y ejecuta el PIE
  empaquetado en `nativeLibraryDir`. No se marca ejecutable ni se ejecuta directamente desde
  `files/`, respetando las restricciones W^X.
- `nativeLibraryDir/`: solo lectura desde la app; contiene el intérprete y, opcionalmente, ffmpeg.

Si falta el manifiesto o cualquier artefacto requerido, el resolvedor devuelve un error explícito.
La ausencia de ffmpeg no invalida una instalación correcta de yt-dlp.
