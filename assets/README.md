# Runtime Python para Android (arm64-v8a)

Android no ejecuta `assets/yt-dlp.bin` ni asume que `libytdlp.so` es un CLI. El runtime soportado
por el código es CPython compilado para Bionic/AArch64 más el zipapp oficial de yt-dlp.

## Estado de los archivos encontrados en el repositorio

- `assets/android-runtime/yt-dlp.pyz` es el zipapp de yt-dlp **2026.08.19**; su SHA-256
  (`1fa6733c...ed8d4d6`) coincide con el asset `yt-dlp` de esa release. Ya está en la ruta que
  usa el instalador Android.
- `jniLibs/arm64-v8a/libpython3.15.so` es una biblioteca compartida ELF AArch64 (`ET_DYN`) con
  `SONAME=libpython3.15.so` y entrypoint `0`. Su ubicación JNI es correcta, pero **no es el
  ejecutable PIE** `libpython3.so` que requiere el runtime. No cambiarle el nombre: Android no
  puede lanzarla como proceso.
- `assets/android-runtime/python/lib/python3.15/` contiene la biblioteca estándar expandida y
  módulos nativos CPython `3.15` para AArch64 Android. Esto satisface el layout de archivos que
  consume el instalador del runtime. Las dependencias de `_ssl`, `_hashlib` y `_sqlite3`
  (`libssl_python.so`, `libcrypto_python.so` y `libsqlite3_python.so`) también están presentes en
  `jniLibs/arm64-v8a/`.
- `bin/yt-dlp` es el ejecutable autónomo Linux AArch64 de yt-dlp. Tiene cargador
  `/lib/ld-linux-aarch64.so.1` (glibc); sirve para Linux AArch64 compatible, **no para Android**.
- `bin/ffmpeg.exe` es el ejecutable Windows x64. `jniLibs/arm64-v8a/libffmpeg.so` era una copia
  con extensión engañosa de ese PE y se retiró de `jniLibs`.
- `jniLibs/arm64-v8a/libytdlp.so` era el mismo ejecutable Linux/glibc AArch64, no una biblioteca
  Android ni un CLI Bionic; se quitó de `jniLibs`.

El archivo `python3_pie` en la raíz es un launcher ELF PIE AArch64 con entrypoint no nulo,
cargador `/system/bin/linker64` y dependencia de `libpython3.15.so`; su fuente está en
`android/launcher.c`. `jniLibs/arm64-v8a/libpython3.so` contiene la copia que usa el APK y
responde a `--version` en un dispositivo ARM64.

`assets/android-runtime/python3.15.zip` es el archivo de biblioteca estándar CPython. Debe estar
instalado como `assets/android-runtime/python/lib/python3.15/python315.zip`; el generador del manifiesto valida
su presencia y las entradas `os.py` y `encodings/__init__.py`. El manifiesto actual enumera los
archivos del runtime y comprueba sus hashes. El script de build también ajusta el filtro AAPT
para no omitir carpetas CPython que comienzan con `_`.

Las bibliotecas MPV/FFmpeg antiguas no constituyen una distribución completa: faltan
dependencias como `libavdevice.so`, `libavfilter.so`, `libavutil.so`, `libswresample.so`,
`libswscale.so` y `libc++_shared.so`. No se ha validado su ejecución ni la feature `mpv`.
No se empaquetan en el APK de descargas: el build selecciona solo el runtime activo en
`target/android-runtime-libs/arm64-v8a`. La firma de release se debe configurar fuera del repositorio, sin guardar contraseñas en
`Cargo.toml`.

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
5. Opcionalmente `jniLibs/arm64-v8a/libffmpeg.so` y `libffprobe.so`: ejecutables PIE Bionic,
   no bibliotecas compartidas ordinarias. Se declaran juntos con `ffmpeg_sha256` y
   `ffprobe_sha256`, y ambos deben pasar hash, comprobación ELF y `-version`.
   yt-dlp recibe la ruta completa a `libffmpeg.so` y descubre su pareja `libffprobe.so`
   por sustitución del nombre. No se necesitan wrappers ejecutables en almacenamiento privado.

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
  "ffprobe_sha256": null
}
```

`python_files` enumera los archivos CPython, incluidos módulos de `lib-dynload` y la biblioteca
estándar ZIP; el ejemplo no es por sí solo un runtime ejecutable. El límite de lectura del
manifiesto en la app es 2 MiB para admitir el listado completo. El intérprete debe aceptar
`PYTHONHOME=<directorio privado>/python`, ejecutar `-S <zipapp> --version` y mostrar exactamente
las versiones declaradas. Al cambiar cualquier artefacto, incrementar `runtime_version`; el
programa conserva las versiones previas y no sobrescribe una versión corrupta con el mismo
identificador.

Con los artefactos ya preparados, genera el JSON y sus hashes desde la raíz del repositorio:

```powershell
.\scripts\Generate-AndroidRuntimeManifest.ps1 `
  -RuntimeVersion cpython-3.15.0-android-arm64-v1 `
  -PythonVersion 3.15.0 `
  -YtDlpVersion 2026.08.19
```

El script requiere `jniLibs/arm64-v8a/libpython3.so`, el árbol expandido
`assets/android-runtime/python/lib/python3.<minor>/` (con `os.py`, `encodings` y módulos `.so` en
`lib-dynload` y `python3<minor>.zip`) y `assets/android-runtime/yt-dlp.pyz`. Para incluir ffmpeg,
añadir `-IncludeFfmpeg` después de empaquetar y revisar `libffmpeg.so`.

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
  `/system/bin/sh <wrapper> <argumentos>`; el wrapper configura `PYTHONHOME` y `SSL_CERT_FILE`
  apunta al bundle PEM que la app genera desde los certificados del sistema Android en
  `/system/etc/security/cacerts`. También conserva esa ruta como `SSL_CERT_DIR` y ejecuta el PIE
  empaquetado en `nativeLibraryDir`. No se marca ejecutable ni se ejecuta directamente desde
  `files/`, respetando las restricciones W^X.
- `files/android-system-ca-bundle.pem`: bundle PEM regenerado desde el almacén CA del sistema
  antes de preparar el wrapper, para que OpenSSL/CPython pueda validar HTTPS con una ruta de
  archivo convencional.
- `nativeLibraryDir/`: solo lectura desde la app; contiene el intérprete y, opcionalmente, ffmpeg.

Si falta el manifiesto o cualquier artefacto requerido, el resolvedor devuelve un error explícito.
La ausencia de ffmpeg no invalida una instalación correcta de yt-dlp.

## Compilar el runtime multimedia

`scripts/Build-FFmpegAndroid.ps1` descarga y verifica FFmpeg 8.0.3 y LAME 3.100, y ejecuta
`android/build_ffmpeg.sh` con Git Bash y el NDK Windows. Visual Studio C++ proporciona los
headers y bibliotecas Windows para las pequeñas utilidades que se ejecutan durante el build;
los ejecutables finales se compilan para Android API 21, no para Windows.

La configuración enlaza las bibliotecas multimedia estáticamente, sin `--enable-gpl` ni
`--enable-nonfree`, e incluye MP3 con LAME, AAC/Opus/Vorbis y contenedores MP4/WebM/Matroska.
FFmpeg procesa archivos locales: la red y TLS permanecen a cargo de CPython/yt-dlp.
Los ejecutables PIE usan alineación máxima de página de 16 KiB y solo bibliotecas del sistema.
No se deben mezclar con las antiguas `libavcodec.so`, `libavformat.so` o `libmpv.so`.

Las fuentes originales y el directorio de compilación se conservan en
`target/ffmpeg-android/`; las licencias LGPL se incorporan a `assets/android-licenses/`.
Antes de distribuir el APK, acompañarlo de las fuentes exactas FFmpeg/LAME, ambas recetas
de compilación, los cambios locales si existieran y las instrucciones de reconstrucción
para cumplir las obligaciones LGPL. No distribuir únicamente los binarios sin ese material.

Después de compilar, regenerar el manifiesto con `-IncludeFfmpeg`. Si una herramienta
declarada falla su validación, se informa el error; no se oculta como si estuviera ausente.
Si no se incluye la pareja, la solicitud de MP3 se rechaza antes de iniciar una descarga.
