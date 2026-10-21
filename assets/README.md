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
  Android no puede lanzarla como proceso. Hace falta un ejecutable bootstrap PIE enlazado con
  esta biblioteca, o una integración Rust/JNI que inicialice CPython dentro del proceso.
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
`android/launcher.c`. Aún no está empaquetado como `jniLibs/arm64-v8a/libpython3.so`.

Por tanto, el zipapp, los archivos expandidos de la biblioteca estándar y las tres bibliotecas
dependientes mencionadas ya están en sus rutas. **Android aún no puede usar el runtime con el
flujo actual**: falta empaquetar el launcher en la ruta esperada y generar el manifiesto.
La integración también debe validarse en un dispositivo antes de considerarse operativa.

Las bibliotecas MPV/FFmpeg incluidas no constituyen una distribución completa: faltan
dependencias como `libavdevice.so`, `libavfilter.so`, `libavutil.so`, `libswresample.so`,
`libswscale.so` y `libc++_shared.so`. No se ha validado su ejecución ni la feature `mpv`.
La firma de release se debe configurar fuera del repositorio, sin guardar contraseñas en
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
5. Opcionalmente `jniLibs/arm64-v8a/libffmpeg.so`: también debe ser un ejecutable PIE Bionic,
   no una biblioteca compartida ordinaria. Se usa solo si `ffmpeg_sha256` está declarado y pasa
   el hash, la comprobación ELF y `ffmpeg -version`.

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
  "ffmpeg_sha256": null
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
  `/system/bin/sh <wrapper> <argumentos>`; el wrapper configura `PYTHONHOME` y ejecuta el PIE
  empaquetado en `nativeLibraryDir`. No se marca ejecutable ni se ejecuta directamente desde
  `files/`, respetando las restricciones W^X.
- `nativeLibraryDir/`: solo lectura desde la app; contiene el intérprete y, opcionalmente, ffmpeg.

Si falta el manifiesto o cualquier artefacto requerido, el resolvedor devuelve un error explícito.
La ausencia de ffmpeg no invalida una instalación correcta de yt-dlp.
