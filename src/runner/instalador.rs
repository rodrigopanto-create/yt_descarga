//! Descarga e instalación segura de binarios publicados en GitHub Releases.
//!
//! Un binario nunca se considera instalado hasta completar, en este orden:
//!
//! 1. Resolución de la etiqueta concreta de la release (`/releases/latest` → `/releases/tag/<tag>`),
//!    para que el artefacto y su `SHA2-256SUMS` provengan de la misma release.
//! 2. Descarga de `SHA2-256SUMS` y extracción del hash del artefacto.
//! 3. Descarga por streaming a un archivo temporal único en el directorio de destino,
//!    calculando SHA-256 al vuelo y aplicando límites de tamaño.
//! 4. `flush()` + `sync_all()` y comparación del hash.
//! 5. Validación estructural del ejecutable (PE / ELF / Mach-O y arquitectura).
//! 6. Ejecución controlada (`--version`) con timeout y salida acotada.
//! 7. `rename` del temporal sobre el destino (mismo directorio). El destino previo no se
//!    elimina nunca antes de ese paso: cualquier fallo lo deja intacto.
//!
//! Procedencia: el hash se obtiene por HTTPS de la misma release de GitHub. Detecta corrupción,
//! truncado y mezclas entre releases, pero NO protege frente a una release comprometida en origen:
//! la firma GPG `SHA2-256SUMS.sig` no se verifica (requeriría una implementación OpenPGP).
//! La ejecución de `--version` es una comprobación funcional, no una garantía de seguridad.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::blocking::{Client, Response};
use reqwest::header::{CONTENT_TYPE, LOCATION};
use reqwest::redirect::Policy;
use ring::digest::{Context, SHA256};

/// Repositorio oficial de yt-dlp.
pub const REPO_YTDLP: &str = "https://github.com/yt-dlp/yt-dlp";
const ARCHIVO_SUMAS: &str = "SHA2-256SUMS";
const MAX_BYTES_SUMAS: u64 = 256 * 1024;
const MAX_BYTES_SALIDA_PROCESO: u64 = 16 * 1024;
const MAX_CARACTERES_MENSAJE: usize = 512;
const TAM_BUFFER: usize = 64 * 1024;
const TAM_CABECERA: u64 = 4096;
/// Los ejecutables autónomos de yt-dlp pesan entre ~13 MiB y ~40 MiB.
const YTDLP_TAMANO_MIN: u64 = 1024 * 1024;
const YTDLP_TAMANO_MAX: u64 = 256 * 1024 * 1024;
/// Los ejecutables PyInstaller se descomprimen en el primer arranque y los antivirus
/// los analizan: en Windows un `--version` en frío puede superar los 30 s.
const YTDLP_TIMEOUT_VERSION: Duration = Duration::from_secs(120);
const YTDLP_TIMEOUT_VERSION_LOCAL: Duration = Duration::from_secs(45);

/// Errores diferenciados del proceso de descarga, verificación e instalación.
#[derive(Debug)]
pub enum ErrorInstalacion {
    PlataformaNoSoportada(String),
    Red(String),
    Http { url: String, estado: u16 },
    RespuestaInvalida(String),
    Escritura(String),
    ArchivoVacio,
    TamanoExcesivo { limite: u64, recibido: u64 },
    TamanoInsuficiente { minimo: u64, recibido: u64 },
    ChecksumAusente(String),
    ChecksumIncorrecto { esperado: String, obtenido: String },
    FormatoInvalido(String),
    ArquitecturaIncompatible(String),
    Validacion(String),
    Instalacion(String),
}

impl fmt::Display for ErrorInstalacion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use ErrorInstalacion::*;
        match self {
            PlataformaNoSoportada(m) => write!(f, "Plataforma no soportada: {m}"),
            Red(m) => write!(f, "Error de red: {m}"),
            Http { url, estado } => write!(f, "Error HTTP {estado} al solicitar {url}"),
            RespuestaInvalida(m) => write!(f, "Respuesta HTTP inválida: {m}"),
            Escritura(m) => write!(f, "Error de escritura: {m}"),
            ArchivoVacio => write!(f, "El archivo descargado está vacío"),
            TamanoExcesivo { limite, recibido } => write!(
                f,
                "Descarga demasiado grande: {recibido} bytes (límite {limite})"
            ),
            TamanoInsuficiente { minimo, recibido } => write!(
                f,
                "Descarga demasiado pequeña: {recibido} bytes (mínimo {minimo})"
            ),
            ChecksumAusente(m) => write!(f, "Checksum no disponible: {m}"),
            ChecksumIncorrecto { esperado, obtenido } => write!(
                f,
                "Checksum SHA-256 incorrecto: esperado {esperado}, obtenido {obtenido}"
            ),
            FormatoInvalido(m) => write!(f, "El archivo no es un ejecutable válido: {m}"),
            ArquitecturaIncompatible(m) => write!(f, "Arquitectura incompatible: {m}"),
            Validacion(m) => write!(f, "Validación del ejecutable fallida: {m}"),
            Instalacion(m) => write!(f, "Error al instalar el binario: {m}"),
        }
    }
}

impl std::error::Error for ErrorInstalacion {}

/// Formato de ejecutable esperado para la plataforma de destino.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatoBinario {
    /// Portable Executable (Windows). `maquina` = campo `Machine` del COFF header.
    Pe { maquina: u16 },
    /// ELF little-endian (Linux). `maquina` = `e_machine`.
    Elf { maquina: u16, clase64: bool },
    /// Mach-O 64 bits (macOS), simple o universal. `cputype` debe estar presente.
    MachO { cputype: u32 },
}

/// Artefacto concreto de una release y nombre con el que se instala localmente.
#[derive(Clone, Copy, Debug)]
pub struct Artefacto {
    pub asset: &'static str,
    pub destino: &'static str,
    pub formato: FormatoBinario,
}

/// Comprobación de ejecución previa a la instalación.
pub struct Verificacion<'a> {
    pub args: &'a [&'a str],
    pub timeout: Duration,
    /// Recibe `(stdout, tag_de_la_release)` y decide si la salida corresponde a la herramienta.
    pub salida_valida: &'a (dyn Fn(&str, &str) -> bool + Sync),
}

/// Parámetros completos de una instalación desde GitHub Releases.
pub struct Solicitud<'a> {
    /// URL base del repositorio, p. ej. `https://github.com/yt-dlp/yt-dlp`.
    pub repo: &'a str,
    pub artefacto: Artefacto,
    pub dir_destino: &'a Path,
    pub tamano_min: u64,
    pub tamano_max: u64,
    pub verificacion: Verificacion<'a>,
}

/// Selecciona el ejecutable autónomo de yt-dlp para el SO y la arquitectura de compilación.
///
/// Se usan los ejecutables autónomos (PyInstaller) y no el zipapp `yt-dlp`, porque este último
/// depende de un intérprete Python externo y no puede validarse estructuralmente.
pub fn artefacto_ytdlp() -> Result<Artefacto, ErrorInstalacion> {
    use FormatoBinario::*;

    if cfg!(target_os = "android") {
        return Err(ErrorInstalacion::PlataformaNoSoportada(
            "yt-dlp no publica ejecutables para Android (Bionic). Los binarios Linux de la \
             release requieren glibc/musl y, con targetSdk >= 29, Android prohíbe ejecutar \
             archivos escritos en directorios de datos de la app (W^X). Empaqueta \
             libytdlp.so en jniLibs."
                .to_string(),
        ));
    }

    let elegido = if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some(("yt-dlp.exe", Pe { maquina: 0x8664 }))
    } else if cfg!(all(target_os = "windows", target_arch = "x86")) {
        Some(("yt-dlp_x86.exe", Pe { maquina: 0x014c }))
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Some(("yt-dlp_arm64.exe", Pe { maquina: 0xaa64 }))
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Some((
            "yt-dlp_macos",
            MachO {
                cputype: 0x0100_0007,
            },
        ))
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some((
            "yt-dlp_macos",
            MachO {
                cputype: 0x0100_000c,
            },
        ))
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "x86_64",
        target_env = "musl"
    )) {
        Some((
            "yt-dlp_musllinux",
            Elf {
                maquina: 62,
                clase64: true,
            },
        ))
    } else if cfg!(all(
        target_os = "linux",
        target_arch = "aarch64",
        target_env = "musl"
    )) {
        Some((
            "yt-dlp_musllinux_aarch64",
            Elf {
                maquina: 183,
                clase64: true,
            },
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some((
            "yt-dlp_linux",
            Elf {
                maquina: 62,
                clase64: true,
            },
        ))
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Some((
            "yt-dlp_linux_aarch64",
            Elf {
                maquina: 183,
                clase64: true,
            },
        ))
    } else {
        None
    };

    let (asset, formato) = elegido.ok_or_else(|| {
        ErrorInstalacion::PlataformaNoSoportada(format!(
            "no hay ejecutable autónomo de yt-dlp para {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
    })?;

    let destino = if cfg!(target_os = "windows") {
        "yt-dlp.exe"
    } else {
        "yt-dlp"
    };

    Ok(Artefacto {
        asset,
        destino,
        formato,
    })
}

/// `yt-dlp --version` imprime exactamente la etiqueta de la release (p. ej. `2026.08.19`).
fn version_ytdlp_coincide(stdout: &str, tag: &str) -> bool {
    stdout.lines().map(str::trim).find(|l| !l.is_empty()) == Some(tag)
}

fn version_ytdlp_parece_valida(stdout: &str) -> bool {
    let linea = match stdout.lines().map(str::trim).find(|l| !l.is_empty()) {
        Some(v) => v,
        None => return false,
    };
    let mut partes = linea.split('.');
    let (Some(y), Some(m), Some(d), None) =
        (partes.next(), partes.next(), partes.next(), partes.next())
    else {
        return false;
    };
    y.len() == 4
        && m.len() == 2
        && d.len() == 2
        && y.chars().all(|c| c.is_ascii_digit())
        && m.chars().all(|c| c.is_ascii_digit())
        && d.chars().all(|c| c.is_ascii_digit())
}

fn formato_ytdlp_ejecutable_host() -> Result<FormatoBinario, ErrorInstalacion> {
    use FormatoBinario::*;

    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Ok(Pe { maquina: 0x8664 })
    } else if cfg!(all(target_os = "windows", target_arch = "x86")) {
        Ok(Pe { maquina: 0x014c })
    } else if cfg!(all(target_os = "windows", target_arch = "aarch64")) {
        Ok(Pe { maquina: 0xaa64 })
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        Ok(MachO {
            cputype: 0x0100_0007,
        })
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Ok(MachO {
            cputype: 0x0100_000c,
        })
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Ok(Elf {
            maquina: 62,
            clase64: true,
        })
    } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
        Ok(Elf {
            maquina: 183,
            clase64: true,
        })
    } else if cfg!(all(target_os = "android", target_arch = "aarch64")) {
        Ok(Elf {
            maquina: 183,
            clase64: true,
        })
    } else {
        Err(ErrorInstalacion::PlataformaNoSoportada(format!(
            "no hay validación de formato para {}-{}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )))
    }
}

/// Valida un ejecutable existente (origen sistema/app/embebido) sin descargarlo.
///
/// Comprueba formato/arquitectura y una ejecución acotada de `--version`.
pub fn validar_ytdlp_existente(ruta: &Path) -> Result<(), ErrorInstalacion> {
    if !ruta.is_file() {
        return Err(ErrorInstalacion::Validacion(format!(
            "no existe un archivo ejecutable en {}",
            ruta.display()
        )));
    }
    let formato = formato_ytdlp_ejecutable_host()?;
    validar_formato(ruta, formato)?;
    let verificacion = Verificacion {
        args: &["--version"],
        timeout: YTDLP_TIMEOUT_VERSION_LOCAL,
        salida_valida: &|stdout, _| version_ytdlp_parece_valida(stdout),
    };
    verificar_ejecucion(ruta, &verificacion, "local")
}

/// Solicitud preconfigurada para instalar yt-dlp en `dir_destino`.
pub fn solicitud_ytdlp(dir_destino: &Path) -> Result<Solicitud<'_>, ErrorInstalacion> {
    Ok(Solicitud {
        repo: REPO_YTDLP,
        artefacto: artefacto_ytdlp()?,
        dir_destino,
        tamano_min: YTDLP_TAMANO_MIN,
        tamano_max: YTDLP_TAMANO_MAX,
        verificacion: Verificacion {
            args: &["--version"],
            timeout: YTDLP_TIMEOUT_VERSION,
            salida_valida: &version_ytdlp_coincide,
        },
    })
}

/// Descarga, verifica e instala el artefacto. Devuelve la ruta final solo tras instalarlo.
pub fn instalar_desde_release(s: &Solicitud<'_>) -> Result<PathBuf, ErrorInstalacion> {
    let repo = s.repo.trim_end_matches('/');
    let cliente = construir_cliente(Policy::limited(10))?;

    let tag = resolver_tag(repo)?;
    let url_base = format!("{repo}/releases/download/{tag}");

    let sumas = descargar_texto(&cliente, &format!("{url_base}/{ARCHIVO_SUMAS}"))?;
    let esperado = buscar_checksum(&sumas, s.artefacto.asset)?;

    let url_asset = format!("{url_base}/{}", s.artefacto.asset);
    let respuesta = abrir_respuesta(&cliente, &url_asset, s.tamano_max)?;

    fs::create_dir_all(s.dir_destino).map_err(|e| {
        ErrorInstalacion::Escritura(format!("no se pudo crear {}: {e}", s.dir_destino.display()))
    })?;
    let destino = s.dir_destino.join(s.artefacto.destino);

    // A partir de aquí, cualquier `?` elimina el temporal mediante `Drop`.
    let mut temporal = ArchivoTemporal::crear(s.dir_destino, s.artefacto.destino)?;
    let obtenido = descargar_a_archivo(respuesta, temporal.archivo()?, s.tamano_min, s.tamano_max)?;
    temporal.cerrar()?;

    if obtenido != esperado {
        return Err(ErrorInstalacion::ChecksumIncorrecto { esperado, obtenido });
    }

    validar_formato(temporal.ruta(), s.artefacto.formato)?;
    marcar_ejecutable(temporal.ruta())?;
    verificar_ejecucion(temporal.ruta(), &s.verificacion, &tag)?;

    temporal.instalar(&destino)?;
    Ok(destino)
}

// -------------------------------------------------------------------------
// HTTP
// -------------------------------------------------------------------------

fn construir_cliente(politica: Policy) -> Result<Client, ErrorInstalacion> {
    Client::builder()
        .redirect(politica)
        // En producción se rechaza cualquier URL o redirección que no sea HTTPS.
        // Las pruebas usan un servidor HTTP local.
        .https_only(!cfg!(test))
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(15 * 60))
        .user_agent(concat!("yt_descarga/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| ErrorInstalacion::Red(format!("no se pudo crear el cliente HTTP: {e}")))
}

/// Obtiene la etiqueta de la última release a partir de la redirección de `/releases/latest`,
/// evitando la API REST (con límite de peticiones anónimas).
fn resolver_tag(repo: &str) -> Result<String, ErrorInstalacion> {
    let url = format!("{repo}/releases/latest");
    let cliente = construir_cliente(Policy::none())?;
    let respuesta = cliente
        .get(&url)
        .send()
        .map_err(|e| ErrorInstalacion::Red(format!("{url}: {e}")))?;

    let estado = respuesta.status();
    if !estado.is_redirection() {
        if estado.is_success() {
            return Err(ErrorInstalacion::RespuestaInvalida(format!(
                "{url} no redirigió a una release concreta"
            )));
        }
        return Err(ErrorInstalacion::Http {
            url,
            estado: estado.as_u16(),
        });
    }

    let ubicacion = respuesta
        .headers()
        .get(LOCATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            ErrorInstalacion::RespuestaInvalida(format!("{url}: redirección sin Location"))
        })?;

    extraer_tag(ubicacion).ok_or_else(|| {
        ErrorInstalacion::RespuestaInvalida(format!(
            "{url}: redirección inesperada a {}",
            recortar(ubicacion)
        ))
    })
}

fn extraer_tag(ubicacion: &str) -> Option<String> {
    const MARCA: &str = "/releases/tag/";
    let inicio = ubicacion.rfind(MARCA)? + MARCA.len();
    let tag = ubicacion[inicio..].split(['?', '#']).next()?;
    tag_valido(tag).then(|| tag.to_string())
}

/// La etiqueta se interpola en URLs: solo se admite un juego de caracteres seguro.
fn tag_valido(tag: &str) -> bool {
    !tag.is_empty()
        && tag.len() <= 64
        && !tag.contains("..")
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'_' || b == b'-')
}

/// Envía la petición y valida estado, tipo de contenido y `Content-Length` antes de leer el cuerpo.
fn abrir_respuesta(cliente: &Client, url: &str, max: u64) -> Result<Response, ErrorInstalacion> {
    let respuesta = cliente
        .get(url)
        .send()
        .map_err(|e| ErrorInstalacion::Red(format!("{url}: {e}")))?
        .error_for_status()
        .map_err(|e| match e.status() {
            Some(estado) => ErrorInstalacion::Http {
                url: url.to_string(),
                estado: estado.as_u16(),
            },
            None => ErrorInstalacion::Red(format!("{url}: {e}")),
        })?;

    let es_html = respuesta
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| {
            ct.trim_start()
                .to_ascii_lowercase()
                .starts_with("text/html")
        });
    if es_html {
        return Err(ErrorInstalacion::RespuestaInvalida(format!(
            "{url} devolvió una página HTML"
        )));
    }

    match respuesta.content_length() {
        Some(0) => Err(ErrorInstalacion::ArchivoVacio),
        Some(declarado) if declarado > max => Err(ErrorInstalacion::TamanoExcesivo {
            limite: max,
            recibido: declarado,
        }),
        _ => Ok(respuesta),
    }
}

fn descargar_texto(cliente: &Client, url: &str) -> Result<String, ErrorInstalacion> {
    let mut respuesta = abrir_respuesta(cliente, url, MAX_BYTES_SUMAS)?;
    let mut contenido = Vec::new();
    transferir(&mut respuesta, &mut contenido, MAX_BYTES_SUMAS, None)?;
    String::from_utf8(contenido)
        .map_err(|_| ErrorInstalacion::RespuestaInvalida(format!("{url} no es texto UTF-8")))
}

/// Transmite el cuerpo al archivo y devuelve su SHA-256 en hexadecimal.
fn descargar_a_archivo(
    mut respuesta: Response,
    archivo: &mut File,
    min: u64,
    max: u64,
) -> Result<String, ErrorInstalacion> {
    let declarado = respuesta.content_length();
    let mut hash = Context::new(&SHA256);
    let total = transferir(&mut respuesta, archivo, max, Some(&mut hash))?;

    if total == 0 {
        return Err(ErrorInstalacion::ArchivoVacio);
    }
    if let Some(declarado) = declarado {
        if declarado != total {
            return Err(ErrorInstalacion::RespuestaInvalida(format!(
                "descarga truncada: {total} de {declarado} bytes"
            )));
        }
    }
    if total < min {
        return Err(ErrorInstalacion::TamanoInsuficiente {
            minimo: min,
            recibido: total,
        });
    }
    Ok(hex(hash.finish().as_ref()))
}

/// Copia por bloques con límite de tamaño, distinguiendo errores de lectura (red) y escritura.
fn transferir(
    origen: &mut impl Read,
    destino: &mut impl Write,
    max: u64,
    mut hash: Option<&mut Context>,
) -> Result<u64, ErrorInstalacion> {
    let mut buffer = vec![0u8; TAM_BUFFER];
    let mut total: u64 = 0;
    loop {
        let leidos = match origen.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                return Err(ErrorInstalacion::Red(format!(
                    "lectura interrumpida tras {total} bytes: {e}"
                )))
            }
        };
        total += leidos as u64;
        if total > max {
            return Err(ErrorInstalacion::TamanoExcesivo {
                limite: max,
                recibido: total,
            });
        }
        let bloque = &buffer[..leidos];
        if let Some(h) = hash.as_deref_mut() {
            h.update(bloque);
        }
        destino
            .write_all(bloque)
            .map_err(|e| ErrorInstalacion::Escritura(e.to_string()))?;
    }
    Ok(total)
}

/// Busca el hash del artefacto en un archivo con formato `sha256sum` (`<hex>  <nombre>`).
fn buscar_checksum(sumas: &str, asset: &str) -> Result<String, ErrorInstalacion> {
    let mut encontrado: Option<&str> = None;
    for linea in sumas.lines() {
        let mut partes = linea.split_whitespace();
        let (Some(hash), Some(nombre), None) = (partes.next(), partes.next(), partes.next()) else {
            continue;
        };
        if nombre.strip_prefix('*').unwrap_or(nombre) != asset {
            continue;
        }
        if hash.len() != 64 || !hash.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ErrorInstalacion::ChecksumAusente(format!(
                "entrada malformada para {asset} en {ARCHIVO_SUMAS}"
            )));
        }
        if encontrado.is_some_and(|previo| !previo.eq_ignore_ascii_case(hash)) {
            return Err(ErrorInstalacion::ChecksumAusente(format!(
                "entradas contradictorias para {asset} en {ARCHIVO_SUMAS}"
            )));
        }
        encontrado = Some(hash);
    }
    encontrado.map(str::to_ascii_lowercase).ok_or_else(|| {
        ErrorInstalacion::ChecksumAusente(format!("{asset} no figura en {ARCHIVO_SUMAS}"))
    })
}

fn hex(bytes: &[u8]) -> String {
    const DIGITOS: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITOS[usize::from(b >> 4)] as char);
        s.push(DIGITOS[usize::from(b & 0x0f)] as char);
    }
    s
}

// -------------------------------------------------------------------------
// Archivo temporal e instalación
// -------------------------------------------------------------------------

static CONTADOR_TEMPORALES: AtomicU64 = AtomicU64::new(0);

/// Archivo temporal en el directorio de destino que se elimina al soltarse salvo que se instale.
struct ArchivoTemporal {
    ruta: PathBuf,
    archivo: Option<File>,
    instalado: bool,
}

impl ArchivoTemporal {
    /// `create_new` (O_CREAT|O_EXCL / CREATE_NEW) falla si la ruta ya existe, incluidos enlaces
    /// simbólicos, por lo que nunca se sigue un enlace plantado por un tercero.
    fn crear(dir: &Path, nombre_final: &str) -> Result<Self, ErrorInstalacion> {
        for _ in 0..16 {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0);
            // El nombre termina igual que el destino: en Windows conserva la extensión `.exe`.
            let ruta = dir.join(format!(
                ".{}-{}-{nanos:08x}.part-{nombre_final}",
                std::process::id(),
                CONTADOR_TEMPORALES.fetch_add(1, Ordering::Relaxed),
            ));

            let mut opciones = OpenOptions::new();
            opciones.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opciones.mode(0o600);
            }

            match opciones.open(&ruta) {
                Ok(archivo) => {
                    return Ok(Self {
                        ruta,
                        archivo: Some(archivo),
                        instalado: false,
                    })
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(ErrorInstalacion::Escritura(format!(
                        "no se pudo crear el temporal {}: {e}",
                        ruta.display()
                    )))
                }
            }
        }
        Err(ErrorInstalacion::Escritura(
            "no se pudo generar un nombre temporal único".to_string(),
        ))
    }

    fn ruta(&self) -> &Path {
        &self.ruta
    }

    fn archivo(&mut self) -> Result<&mut File, ErrorInstalacion> {
        self.archivo
            .as_mut()
            .ok_or_else(|| ErrorInstalacion::Escritura("el temporal ya está cerrado".to_string()))
    }

    /// Vuelca a disco y libera el descriptor (Windows no permite ejecutar ni renombrar
    /// un archivo con un handle de escritura abierto).
    fn cerrar(&mut self) -> Result<(), ErrorInstalacion> {
        if let Some(mut archivo) = self.archivo.take() {
            archivo
                .flush()
                .and_then(|_| archivo.sync_all())
                .map_err(|e| ErrorInstalacion::Escritura(format!("al sincronizar: {e}")))?;
        }
        Ok(())
    }

    fn instalar(mut self, destino: &Path) -> Result<(), ErrorInstalacion> {
        self.cerrar()?;
        reemplazar(&self.ruta, destino).map_err(|e| {
            ErrorInstalacion::Instalacion(format!(
                "no se pudo mover {} a {}: {e}",
                self.ruta.display(),
                destino.display()
            ))
        })?;
        self.instalado = true;
        sincronizar_directorio(destino);
        Ok(())
    }
}

impl Drop for ArchivoTemporal {
    fn drop(&mut self) {
        if self.instalado {
            return;
        }
        drop(self.archivo.take());
        if let Err(e) = fs::remove_file(&self.ruta) {
            if e.kind() != io::ErrorKind::NotFound {
                log::warn!(
                    "No se pudo eliminar el temporal {}: {e}",
                    self.ruta.display()
                );
            }
        }
    }
}

/// Reemplazo del destino por el temporal (mismo directorio, mismo sistema de archivos).
///
/// - Unix: `rename(2)` sustituye el destino de forma atómica.
/// - Windows: `std::fs::rename` sustituye el destino existente sin borrarlo antes; si el destino
///   está en uso (p. ej. yt-dlp ejecutándose) falla y la versión previa queda intacta.
fn reemplazar(origen: &Path, destino: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Antivirus e indexadores abren brevemente los ejecutables recién escritos.
        let mut intentos = 0;
        loop {
            match fs::rename(origen, destino) {
                Err(e) if e.kind() == io::ErrorKind::PermissionDenied && intentos < 5 => {
                    intentos += 1;
                    thread::sleep(Duration::from_millis(200));
                }
                resultado => return resultado,
            }
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(origen, destino)
    }
}

/// Persiste la entrada de directorio tras el `rename` (solo Unix; best effort).
fn sincronizar_directorio(destino: &Path) {
    #[cfg(unix)]
    {
        if let Some(dir) = destino.parent() {
            if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
                log::warn!("No se pudo sincronizar {}: {e}", dir.display());
            }
        }
    }
    #[cfg(not(unix))]
    let _ = destino;
}

fn marcar_ejecutable(ruta: &Path) -> Result<(), ErrorInstalacion> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(ruta, fs::Permissions::from_mode(0o755)).map_err(|e| {
            ErrorInstalacion::Escritura(format!("no se pudieron aplicar permisos: {e}"))
        })?;
    }
    #[cfg(not(unix))]
    let _ = ruta;
    Ok(())
}

// -------------------------------------------------------------------------
// Validación estructural
// -------------------------------------------------------------------------

fn validar_formato(ruta: &Path, esperado: FormatoBinario) -> Result<(), ErrorInstalacion> {
    let mut cabecera = Vec::with_capacity(TAM_CABECERA as usize);
    File::open(ruta)
        .and_then(|f| f.take(TAM_CABECERA).read_to_end(&mut cabecera))
        .map_err(|e| ErrorInstalacion::Validacion(format!("no se pudo leer la cabecera: {e}")))?;
    validar_cabecera(&cabecera, esperado)
}

fn leer_u16_le(c: &[u8], off: usize) -> Option<u16> {
    c.get(off..off.checked_add(2)?)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
}

fn leer_u32_le(c: &[u8], off: usize) -> Option<u32> {
    c.get(off..off.checked_add(4)?)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn leer_u32_be(c: &[u8], off: usize) -> Option<u32> {
    c.get(off..off.checked_add(4)?)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn validar_cabecera(c: &[u8], esperado: FormatoBinario) -> Result<(), ErrorInstalacion> {
    let invalido = |m: &str| ErrorInstalacion::FormatoInvalido(m.to_string());
    match esperado {
        FormatoBinario::Pe { maquina } => {
            if !c.starts_with(b"MZ") {
                return Err(invalido("falta la firma MZ de un ejecutable PE"));
            }
            let pe =
                leer_u32_le(c, 0x3c).ok_or_else(|| invalido("cabecera DOS truncada"))? as usize;
            if c.get(pe..pe.saturating_add(4)) != Some(b"PE\0\0".as_slice()) {
                return Err(invalido("falta la firma PE"));
            }
            let real = leer_u16_le(c, pe + 4).ok_or_else(|| invalido("cabecera COFF truncada"))?;
            let caracteristicas =
                leer_u16_le(c, pe + 22).ok_or_else(|| invalido("cabecera COFF truncada"))?;
            // IMAGE_FILE_EXECUTABLE_IMAGE presente e IMAGE_FILE_DLL ausente.
            if caracteristicas & 0x0002 == 0 || caracteristicas & 0x2000 != 0 {
                return Err(invalido("la imagen PE no es un ejecutable"));
            }
            if real != maquina {
                return Err(ErrorInstalacion::ArquitecturaIncompatible(format!(
                    "PE para máquina 0x{real:04x}, se esperaba 0x{maquina:04x}"
                )));
            }
        }
        FormatoBinario::Elf { maquina, clase64 } => {
            if !c.starts_with(b"\x7fELF") {
                return Err(invalido("falta la firma ELF"));
            }
            let clase = c.get(4).copied();
            if clase != Some(if clase64 { 2 } else { 1 }) {
                return Err(ErrorInstalacion::ArquitecturaIncompatible(format!(
                    "clase ELF {clase:?} no coincide con la esperada"
                )));
            }
            if c.get(5) != Some(&1) {
                return Err(ErrorInstalacion::ArquitecturaIncompatible(
                    "ELF no little-endian".to_string(),
                ));
            }
            let tipo = leer_u16_le(c, 16).ok_or_else(|| invalido("cabecera ELF truncada"))?;
            if tipo != 2 && tipo != 3 {
                return Err(invalido("el ELF no es un ejecutable (ET_EXEC/ET_DYN)"));
            }
            let real = leer_u16_le(c, 18).ok_or_else(|| invalido("cabecera ELF truncada"))?;
            if real != maquina {
                return Err(ErrorInstalacion::ArquitecturaIncompatible(format!(
                    "ELF para e_machine {real}, se esperaba {maquina}"
                )));
            }
        }
        FormatoBinario::MachO { cputype } => {
            let cpus: Vec<u32> = if c.starts_with(&[0xcf, 0xfa, 0xed, 0xfe]) {
                // MH_EXECUTE = 2
                if leer_u32_le(c, 12) != Some(2) {
                    return Err(invalido("el Mach-O no es un ejecutable"));
                }
                leer_u32_le(c, 4).into_iter().collect()
            } else if c.starts_with(&[0xca, 0xfe, 0xba, 0xbe]) {
                let n = leer_u32_be(c, 4).ok_or_else(|| invalido("cabecera fat truncada"))?;
                // Acota `n` para no confundir un `.class` de Java (misma firma) con un binario universal.
                if n == 0 || n > 16 {
                    return Err(invalido("número de arquitecturas Mach-O inválido"));
                }
                (0..n as usize)
                    .map(|i| leer_u32_be(c, 8 + i * 20))
                    .collect::<Option<Vec<_>>>()
                    .ok_or_else(|| invalido("cabecera fat truncada"))?
            } else {
                return Err(invalido("falta la firma Mach-O de 64 bits"));
            };
            if !cpus.contains(&cputype) {
                return Err(ErrorInstalacion::ArquitecturaIncompatible(format!(
                    "Mach-O para cputype {cpus:x?}, se esperaba 0x{cputype:x}"
                )));
            }
        }
    }
    Ok(())
}

// -------------------------------------------------------------------------
// Ejecución controlada
// -------------------------------------------------------------------------

pub(crate) struct SalidaProceso {
    pub(crate) estado: ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

fn verificar_ejecucion(
    ruta: &Path,
    v: &Verificacion<'_>,
    tag: &str,
) -> Result<(), ErrorInstalacion> {
    let mut cmd = Command::new(ruta);
    cmd.args(v.args);
    let salida = ejecutar_con_timeout(cmd, v.timeout)?;

    if !salida.estado.success() {
        return Err(ErrorInstalacion::Validacion(format!(
            "terminó con {}; stderr: {}",
            salida.estado,
            recortar(salida.stderr.trim())
        )));
    }
    if !(v.salida_valida)(&salida.stdout, tag) {
        return Err(ErrorInstalacion::Validacion(format!(
            "la salida no corresponde a la herramienta esperada (release {tag}): {}",
            recortar(salida.stdout.trim())
        )));
    }
    Ok(())
}

/// Ejecuta `cmd` con stdin cerrado, salida acotada y un plazo máximo; al vencer, mata el proceso.
pub(crate) fn ejecutar_con_timeout(
    mut cmd: Command,
    timeout: Duration,
) -> Result<SalidaProceso, ErrorInstalacion> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut hijo = cmd.spawn().map_err(|e| {
        ErrorInstalacion::Validacion(format!("no se pudo ejecutar en esta plataforma: {e}"))
    })?;

    let rx_stdout = leer_acotado(hijo.stdout.take());
    let rx_stderr = leer_acotado(hijo.stderr.take());

    let limite = Instant::now() + timeout;
    let estado = loop {
        match hijo.try_wait() {
            Ok(Some(estado)) => break estado,
            Ok(None) if Instant::now() < limite => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                // Errores de kill/wait irrelevantes: el proceso ya se descarta.
                let _ = hijo.kill();
                let _ = hijo.wait();
                return Err(ErrorInstalacion::Validacion(format!(
                    "sin respuesta tras {} s; proceso terminado",
                    timeout.as_secs_f32()
                )));
            }
            Err(e) => {
                let _ = hijo.kill();
                let _ = hijo.wait();
                return Err(ErrorInstalacion::Validacion(format!(
                    "no se pudo esperar al proceso: {e}"
                )));
            }
        }
    };

    // Un nieto que herede los pipes podría mantenerlos abiertos: se espera un tiempo acotado.
    let espera = Duration::from_secs(2);
    Ok(SalidaProceso {
        estado,
        stdout: rx_stdout.recv_timeout(espera).unwrap_or_default(),
        stderr: rx_stderr.recv_timeout(espera).unwrap_or_default(),
    })
}

/// Lee como máximo `MAX_BYTES_SALIDA_PROCESO` en un hilo y descarta el resto sin retenerlo,
/// para que el proceso hijo no se bloquee con el pipe lleno.
fn leer_acotado<R: Read + Send + 'static>(pipe: Option<R>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    if let Some(mut pipe) = pipe {
        thread::spawn(move || {
            let mut buffer = Vec::new();
            let _ = (&mut pipe)
                .take(MAX_BYTES_SALIDA_PROCESO)
                .read_to_end(&mut buffer);
            let _ = tx.send(String::from_utf8_lossy(&buffer).into_owned());
            let _ = io::copy(&mut pipe, &mut io::sink());
        });
    }
    rx
}

fn recortar(texto: &str) -> String {
    match texto.char_indices().nth(MAX_CARACTERES_MENSAJE) {
        Some((corte, _)) => format!("{}…", &texto[..corte]),
        None => texto.to_string(),
    }
}

// =========================================================================
// PRUEBAS (sin Internet: servidor HTTP local mínimo sobre TcpListener)
// =========================================================================
#[cfg(test)]
mod pruebas {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Barrier, OnceLock};

    const TAG: &str = "2026.01.02";
    const ASSET: &str = "herramienta-prueba";
    const DESTINO: &str = if cfg!(windows) {
        "herramienta.exe"
    } else {
        "herramienta"
    };
    const VAR_BLOQUEO: &str = "YT_DESCARGA_PRUEBA_BLOQUEO";

    #[derive(Clone)]
    enum Resp {
        Ok(Arc<[u8]>),
        Estado(u16),
        Redireccion(String),
        Html,
        Truncada { declarado: usize, cuerpo: Vec<u8> },
    }

    fn servidor(rutas: Vec<(String, Resp)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let base = format!("http://{}", listener.local_addr().expect("addr"));
        let rutas: Arc<HashMap<String, Resp>> = Arc::new(rutas.into_iter().collect());
        thread::spawn(move || {
            for conexion in listener.incoming().flatten() {
                let rutas = Arc::clone(&rutas);
                thread::spawn(move || atender(conexion, &rutas));
            }
        });
        base
    }

    fn atender(mut s: TcpStream, rutas: &HashMap<String, Resp>) {
        let Ok(lectura) = s.try_clone() else { return };
        let mut lector = BufReader::new(lectura);
        let mut linea = String::new();
        if lector.read_line(&mut linea).is_err() {
            return;
        }
        let ruta = linea.split_whitespace().nth(1).unwrap_or("").to_string();
        loop {
            let mut cabecera = String::new();
            match lector.read_line(&mut cabecera) {
                Ok(0) | Err(_) => break,
                Ok(_) if cabecera == "\r\n" => break,
                Ok(_) => {}
            }
        }

        let escribir = |s: &mut TcpStream, codigo: u16, extra: &str, len: usize, cuerpo: &[u8]| {
            let cab = format!(
                "HTTP/1.1 {codigo} X\r\nContent-Length: {len}\r\n{extra}Connection: close\r\n\r\n"
            );
            let _ = s.write_all(cab.as_bytes());
            let _ = s.write_all(cuerpo);
            let _ = s.flush();
        };
        match rutas.get(&ruta).cloned().unwrap_or(Resp::Estado(404)) {
            Resp::Ok(c) => escribir(
                &mut s,
                200,
                "Content-Type: application/octet-stream\r\n",
                c.len(),
                &c,
            ),
            Resp::Estado(codigo) => escribir(&mut s, codigo, "", 5, b"error"),
            Resp::Redireccion(destino) => {
                escribir(&mut s, 302, &format!("Location: {destino}\r\n"), 0, b"")
            }
            Resp::Html => escribir(
                &mut s,
                200,
                "Content-Type: text/html\r\n",
                13,
                b"<html></html>",
            ),
            Resp::Truncada { declarado, cuerpo } => escribir(&mut s, 200, "", declarado, &cuerpo),
        }
    }

    fn sha256(datos: &[u8]) -> String {
        hex(ring::digest::digest(&SHA256, datos).as_ref())
    }

    fn sumas_para(datos: &[u8]) -> String {
        format!(
            "{}  otro-asset\n{}  {ASSET}\n",
            "0".repeat(64),
            sha256(datos)
        )
    }

    fn release(sumas: Resp, asset: Resp) -> String {
        servidor(vec![
            (
                "/releases/latest".to_string(),
                Resp::Redireccion(format!("/o/r/releases/tag/{TAG}")),
            ),
            (format!("/releases/download/{TAG}/{ARCHIVO_SUMAS}"), sumas),
            (format!("/releases/download/{TAG}/{ASSET}"), asset),
        ])
    }

    fn release_valida(contenido: Arc<[u8]>) -> String {
        release(
            Resp::Ok(sumas_para(&contenido).into_bytes().into()),
            Resp::Ok(contenido),
        )
    }

    /// El propio binario de pruebas es un ejecutable real para el SO/arquitectura del host.
    fn ejecutable_host() -> Arc<[u8]> {
        static EXE: OnceLock<Arc<[u8]>> = OnceLock::new();
        Arc::clone(EXE.get_or_init(|| {
            let ruta = std::env::current_exe().expect("current_exe");
            fs::read(ruta).expect("leer current_exe").into()
        }))
    }

    fn formato_host() -> FormatoBinario {
        artefacto_ytdlp()
            .expect("host de pruebas soportado")
            .formato
    }

    fn salida_harness(stdout: &str, _tag: &str) -> bool {
        stdout.contains("instalacion_valida_e_integra")
    }

    fn rechazar_salida(_: &str, _: &str) -> bool {
        false
    }

    fn solicitud<'a>(repo: &'a str, dir: &'a Path) -> Solicitud<'a> {
        Solicitud {
            repo,
            artefacto: Artefacto {
                asset: ASSET,
                destino: DESTINO,
                formato: formato_host(),
            },
            dir_destino: dir,
            tamano_min: 1,
            tamano_max: 1 << 30,
            verificacion: Verificacion {
                args: &["--list"],
                timeout: Duration::from_secs(120),
                salida_valida: &salida_harness,
            },
        }
    }

    struct DirPrueba(PathBuf);

    impl DirPrueba {
        fn nuevo(nombre: &str) -> Self {
            static N: AtomicU64 = AtomicU64::new(0);
            let ruta = std::env::temp_dir().join(format!(
                "yt_descarga_pruebas_{nombre}_{}_{}",
                std::process::id(),
                N.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&ruta);
            fs::create_dir_all(&ruta).expect("crear dir de prueba");
            Self(ruta)
        }
    }

    impl Drop for DirPrueba {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temporales(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .map(|it| {
                it.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .filter(|n| n.contains(".part-"))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn assert_sin_rastro(dir: &Path) {
        assert!(
            temporales(dir).is_empty(),
            "temporales: {:?}",
            temporales(dir)
        );
        assert!(!dir.join(DESTINO).exists(), "no debe instalarse nada");
    }

    fn parchear_arquitectura(original: &[u8]) -> Arc<[u8]> {
        let mut b = original.to_vec();
        match formato_host() {
            FormatoBinario::Pe { .. } => {
                let pe = leer_u32_le(&b, 0x3c).expect("e_lfanew") as usize;
                b[pe + 4..pe + 6].copy_from_slice(&0x01c4u16.to_le_bytes()); // ARMv7 (ARMNT)
            }
            FormatoBinario::Elf { .. } => b[18..20].copy_from_slice(&243u16.to_le_bytes()), // RISC-V
            FormatoBinario::MachO { .. } => b[4..8].copy_from_slice(&0x0100_0012u32.to_le_bytes()), // PPC64
        }
        b.into()
    }

    /// Proceso auxiliar para la prueba de timeout: solo se bloquea si lo lanza esa prueba.
    #[test]
    fn auxiliar_bloqueo() {
        if std::env::var_os(VAR_BLOQUEO).is_some() {
            thread::sleep(Duration::from_secs(120));
        }
    }

    // 1. HTTP 404 / 500 --------------------------------------------------------

    #[test]
    fn http_404_en_artefacto() {
        let dir = DirPrueba::nuevo("404");
        let base = release(
            Resp::Ok(sumas_para(b"x").into_bytes().into()),
            Resp::Estado(404),
        );
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::Http { estado: 404, .. })),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn http_500_en_checksums() {
        let dir = DirPrueba::nuevo("500");
        let base = release(Resp::Estado(500), Resp::Ok(ejecutable_host()));
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::Http { estado: 500, .. })),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn pagina_html_rechazada() {
        let dir = DirPrueba::nuevo("html");
        let base = release(Resp::Ok(sumas_para(b"x").into_bytes().into()), Resp::Html);
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::RespuestaInvalida(_))),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    // 2. Errores de red ------------------------------------------------------

    #[test]
    fn conexion_rechazada() {
        let dir = DirPrueba::nuevo("red");
        let puerto = TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .expect("puerto libre")
            .port();
        let base = format!("http://127.0.0.1:{puerto}");
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(matches!(r, Err(ErrorInstalacion::Red(_))), "{r:?}");
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn corte_de_conexion_durante_la_descarga() {
        let dir = DirPrueba::nuevo("corte");
        let cuerpo = vec![7u8; 1000];
        let base = release(
            Resp::Ok(sumas_para(&[7u8; 100_000]).into_bytes().into()),
            Resp::Truncada {
                declarado: 100_000,
                cuerpo,
            },
        );
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(
                r,
                Err(ErrorInstalacion::Red(_)) | Err(ErrorInstalacion::RespuestaInvalida(_))
            ),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    // 3. Respuesta vacía y límites de tamaño ---------------------------------

    #[test]
    fn respuesta_vacia() {
        let dir = DirPrueba::nuevo("vacia");
        let base = release(
            Resp::Ok(sumas_para(b"").into_bytes().into()),
            Resp::Ok(Arc::from(Vec::new())),
        );
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(matches!(r, Err(ErrorInstalacion::ArchivoVacio)), "{r:?}");
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn tamano_excesivo_y_insuficiente() {
        let dir = DirPrueba::nuevo("tamano");
        let datos: Arc<[u8]> = vec![1u8; 200].into();
        let base = release_valida(Arc::clone(&datos));

        let mut s = solicitud(&base, &dir.0);
        s.tamano_max = 100;
        let r = instalar_desde_release(&s);
        assert!(
            matches!(r, Err(ErrorInstalacion::TamanoExcesivo { .. })),
            "{r:?}"
        );

        let mut s = solicitud(&base, &dir.0);
        s.tamano_min = 1000;
        let r = instalar_desde_release(&s);
        assert!(
            matches!(r, Err(ErrorInstalacion::TamanoInsuficiente { .. })),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    // 4. Checksum -------------------------------------------------------------

    #[test]
    fn checksum_incorrecto() {
        let dir = DirPrueba::nuevo("sha_mal");
        let base = release(
            Resp::Ok(sumas_para(b"otro contenido").into_bytes().into()),
            Resp::Ok(ejecutable_host()),
        );
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::ChecksumIncorrecto { .. })),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn checksum_ausente() {
        let dir = DirPrueba::nuevo("sha_ausente");
        let sumas = format!("{}  otro-asset\n", "a".repeat(64));
        let base = release(
            Resp::Ok(sumas.into_bytes().into()),
            Resp::Ok(ejecutable_host()),
        );
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::ChecksumAusente(_))),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn parseo_de_checksums() {
        let h = "AB".repeat(32);
        assert_eq!(
            buscar_checksum(&format!("{h} *{ASSET}\n"), ASSET).ok(),
            Some(h.to_ascii_lowercase())
        );
        assert!(buscar_checksum(&format!("zz  {ASSET}\n"), ASSET).is_err());
        let contradictorio = format!("{h}  {ASSET}\n{}  {ASSET}\n", "cd".repeat(32));
        assert!(buscar_checksum(&contradictorio, ASSET).is_err());
        assert!(buscar_checksum(&format!("{h}  {ASSET}.zip\n"), ASSET).is_err());
    }

    #[test]
    fn extraccion_de_tag() {
        assert_eq!(
            extraer_tag("https://github.com/yt-dlp/yt-dlp/releases/tag/2026.08.19").as_deref(),
            Some("2026.08.19")
        );
        assert_eq!(
            extraer_tag("/x/releases/tag/v1.0?a=b").as_deref(),
            Some("v1.0")
        );
        assert!(extraer_tag("/x/releases/tag/../../evil").is_none());
        assert!(extraer_tag("/x/releases/tag/a%2Fb").is_none());
        assert!(extraer_tag("/login").is_none());
    }

    // 5. Fallo al crear el temporal -------------------------------------------

    #[test]
    fn fallo_al_crear_directorio_o_temporal() {
        let dir = DirPrueba::nuevo("escritura");
        let archivo = dir.0.join("no-soy-directorio");
        fs::write(&archivo, b"x").expect("crear archivo");
        let destino = archivo.join("sub");
        let base = release_valida(ejecutable_host());
        let r = instalar_desde_release(&solicitud(&base, &destino));
        assert!(matches!(r, Err(ErrorInstalacion::Escritura(_))), "{r:?}");
        assert!(temporales(&dir.0).is_empty());
    }

    // 6. Ejecutable inválido / arquitectura / validación de ejecución ---------

    #[test]
    fn contenido_no_ejecutable() {
        let dir = DirPrueba::nuevo("no_exe");
        let base = release_valida(Arc::from(b"#!/bin/sh\necho hola\n".to_vec()));
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::FormatoInvalido(_))),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn arquitectura_incompatible() {
        let dir = DirPrueba::nuevo("arch");
        let base = release_valida(parchear_arquitectura(&ejecutable_host()));
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(
            matches!(r, Err(ErrorInstalacion::ArquitecturaIncompatible(_))),
            "{r:?}"
        );
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn salida_de_version_inesperada() {
        let dir = DirPrueba::nuevo("salida");
        let base = release_valida(ejecutable_host());
        let mut s = solicitud(&base, &dir.0);
        s.verificacion.salida_valida = &rechazar_salida;
        let r = instalar_desde_release(&s);
        assert!(matches!(r, Err(ErrorInstalacion::Validacion(_))), "{r:?}");
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn codigo_de_salida_no_cero() {
        let dir = DirPrueba::nuevo("exit");
        let base = release_valida(ejecutable_host());
        let mut s = solicitud(&base, &dir.0);
        s.verificacion.args = &["--opcion-que-no-existe"];
        let r = instalar_desde_release(&s);
        assert!(matches!(r, Err(ErrorInstalacion::Validacion(_))), "{r:?}");
        assert_sin_rastro(&dir.0);
    }

    #[test]
    fn timeout_mata_el_proceso() {
        let exe = std::env::current_exe().expect("current_exe");
        let mut cmd = Command::new(exe);
        cmd.args(["auxiliar_bloqueo", "--nocapture", "--test-threads=1"])
            .env(VAR_BLOQUEO, "1");
        let inicio = Instant::now();
        let r = ejecutar_con_timeout(cmd, Duration::from_secs(2));
        assert!(
            matches!(&r, Err(ErrorInstalacion::Validacion(m)) if m.contains("sin respuesta")),
            "{:?}",
            r.err()
        );
        assert!(inicio.elapsed() < Duration::from_secs(30));
    }

    #[test]
    fn cabeceras_sinteticas() {
        let mut pe = vec![0u8; 0x200];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        pe[0x80..0x84].copy_from_slice(b"PE\0\0");
        pe[0x84..0x86].copy_from_slice(&0x8664u16.to_le_bytes());
        pe[0x96..0x98].copy_from_slice(&0x0022u16.to_le_bytes());
        assert!(validar_cabecera(&pe, FormatoBinario::Pe { maquina: 0x8664 }).is_ok());
        assert!(matches!(
            validar_cabecera(&pe, FormatoBinario::Pe { maquina: 0xaa64 }),
            Err(ErrorInstalacion::ArquitecturaIncompatible(_))
        ));
        pe[0x96..0x98].copy_from_slice(&0x2022u16.to_le_bytes()); // DLL
        assert!(validar_cabecera(&pe, FormatoBinario::Pe { maquina: 0x8664 }).is_err());

        let mut elf = vec![0u8; 64];
        elf[..4].copy_from_slice(b"\x7fELF");
        elf[4] = 2;
        elf[5] = 1;
        elf[16..18].copy_from_slice(&3u16.to_le_bytes());
        elf[18..20].copy_from_slice(&183u16.to_le_bytes());
        let aarch64 = FormatoBinario::Elf {
            maquina: 183,
            clase64: true,
        };
        assert!(validar_cabecera(&elf, aarch64).is_ok());
        assert!(matches!(
            validar_cabecera(
                &elf,
                FormatoBinario::Elf {
                    maquina: 62,
                    clase64: true
                }
            ),
            Err(ErrorInstalacion::ArquitecturaIncompatible(_))
        ));

        let mut fat = vec![0u8; 64];
        fat[..4].copy_from_slice(&[0xca, 0xfe, 0xba, 0xbe]);
        fat[4..8].copy_from_slice(&2u32.to_be_bytes());
        fat[8..12].copy_from_slice(&0x0100_0007u32.to_be_bytes());
        fat[28..32].copy_from_slice(&0x0100_000cu32.to_be_bytes());
        assert!(validar_cabecera(
            &fat,
            FormatoBinario::MachO {
                cputype: 0x0100_000c
            }
        )
        .is_ok());
        assert!(validar_cabecera(b"<html>", FormatoBinario::MachO { cputype: 7 }).is_err());
    }

    // 7. Fallo durante la instalación final -----------------------------------

    #[test]
    fn fallo_en_la_instalacion_final() {
        let dir = DirPrueba::nuevo("rename");
        // Un directorio no vacío en la ruta de destino hace fallar el `rename` en todas las plataformas.
        let bloqueo = dir.0.join(DESTINO);
        fs::create_dir_all(&bloqueo).expect("crear dir");
        fs::write(bloqueo.join("dato"), b"intacto").expect("escribir");

        let base = release_valida(ejecutable_host());
        let r = instalar_desde_release(&solicitud(&base, &dir.0));
        assert!(matches!(r, Err(ErrorInstalacion::Instalacion(_))), "{r:?}");
        assert!(temporales(&dir.0).is_empty());
        assert_eq!(
            fs::read(bloqueo.join("dato")).ok().as_deref(),
            Some(&b"intacto"[..])
        );
    }

    // 8. Conservación del binario anterior ------------------------------------

    #[test]
    fn conserva_el_binario_anterior_si_la_actualizacion_falla() {
        let dir = DirPrueba::nuevo("anterior");
        let destino = dir.0.join(DESTINO);
        fs::write(&destino, b"version-anterior").expect("escribir anterior");

        let casos = [
            release(
                Resp::Ok(sumas_para(b"otro").into_bytes().into()),
                Resp::Ok(ejecutable_host()),
            ),
            release_valida(parchear_arquitectura(&ejecutable_host())),
            release(
                Resp::Ok(sumas_para(b"x").into_bytes().into()),
                Resp::Estado(500),
            ),
        ];
        for base in &casos {
            assert!(instalar_desde_release(&solicitud(base, &dir.0)).is_err());
            assert_eq!(
                fs::read(&destino).ok().as_deref(),
                Some(&b"version-anterior"[..])
            );
            assert!(temporales(&dir.0).is_empty());
        }
    }

    // 9. Concurrencia ----------------------------------------------------------

    #[test]
    fn instalaciones_concurrentes_validas() {
        let dir = DirPrueba::nuevo("concurrente");
        let exe = ejecutable_host();
        let base = release_valida(Arc::clone(&exe));
        let barrera = Barrier::new(2);

        let resultados: Vec<_> = thread::scope(|ambito| {
            let hilos: Vec<_> = (0..2)
                .map(|_| {
                    ambito.spawn(|| {
                        barrera.wait();
                        instalar_desde_release(&solicitud(&base, &dir.0))
                    })
                })
                .collect();
            hilos.into_iter().map(|h| h.join().expect("hilo")).collect()
        });

        for r in &resultados {
            assert!(r.is_ok(), "{r:?}");
        }
        assert_eq!(fs::read(dir.0.join(DESTINO)).expect("instalado"), &exe[..]);
        assert!(temporales(&dir.0).is_empty());
    }

    #[test]
    fn instalacion_invalida_concurrente_no_pisa_la_valida() {
        let dir = DirPrueba::nuevo("concurrente_mixta");
        let exe = ejecutable_host();
        let valida = release_valida(Arc::clone(&exe));
        let invalida = release(
            Resp::Ok(sumas_para(b"otro").into_bytes().into()),
            Resp::Ok(parchear_arquitectura(&exe)),
        );
        let barrera = Barrier::new(2);

        let (rv, ri) = thread::scope(|ambito| {
            let hv = ambito.spawn(|| {
                barrera.wait();
                instalar_desde_release(&solicitud(&valida, &dir.0))
            });
            let hi = ambito.spawn(|| {
                barrera.wait();
                instalar_desde_release(&solicitud(&invalida, &dir.0))
            });
            (hv.join().expect("hilo"), hi.join().expect("hilo"))
        });

        assert!(rv.is_ok(), "{rv:?}");
        assert!(ri.is_err());
        assert_eq!(fs::read(dir.0.join(DESTINO)).expect("instalado"), &exe[..]);
        assert!(temporales(&dir.0).is_empty());
    }

    // 10. Instalación válida ---------------------------------------------------

    #[test]
    fn instalacion_valida_e_integra() {
        let dir = DirPrueba::nuevo("valida");
        let exe = ejecutable_host();
        let destino = dir.0.join(DESTINO);
        fs::write(&destino, b"version-anterior").expect("escribir anterior");

        let base = release_valida(Arc::clone(&exe));
        let ruta = instalar_desde_release(&solicitud(&base, &dir.0)).expect("instalación");

        assert_eq!(ruta, destino);
        assert_eq!(fs::read(&ruta).expect("leer"), &exe[..]);
        assert!(temporales(&dir.0).is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let modo = fs::metadata(&ruta).expect("metadata").permissions().mode();
            assert_eq!(modo & 0o111, 0o111);
        }
    }

    #[test]
    fn version_ytdlp_debe_coincidir_con_el_tag() {
        assert!(version_ytdlp_coincide("2026.08.19\n", "2026.08.19"));
        assert!(!version_ytdlp_coincide("2026.08.18\n", "2026.08.19"));
        assert!(!version_ytdlp_coincide("", "2026.08.19"));
    }
}
