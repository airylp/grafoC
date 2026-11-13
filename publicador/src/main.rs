//! grafo-publicador: sirve una página para publicar y descargar los ejecutables de grafo.
//!
//! Variables de entorno:
//!   GRAFO_ESCUCHA  dirección y puerto (por defecto 127.0.0.1:8097; el servicio usa la IP de Tailscale)
//!   GRAFO_DATOS    carpeta donde se guardan las versiones (por defecto ~/.local/share/grafo-publicador)
//!   GRAFO_FUENTE   carpeta del código de grafo (por defecto la carpeta padre de este programa)
//!
//! Para publicar o borrar hace falta el token guardado en `$GRAFO_DATOS/token`
//! (se crea solo la primera vez). Descargar no lo requiere.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Path as Ruta, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{delete, get, post},
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio_util::io::ReaderStream;

const PAGINA: &str = include_str!("../web/index.html");
const INSTALAR: &str = include_str!("../web/instalar.sh");
const DESCARGAR: &str = include_str!("../web/descargar.sh");
/// Tamaño máximo de un ejecutable subido.
const MAX_SUBIDA: usize = 1024 * 1024 * 1024;

struct Estado {
    datos: PathBuf,
    fuente: PathBuf,
    token: String,
}

type Compartido = Arc<Estado>;

#[derive(Serialize, Deserialize, Clone)]
struct Version {
    id: String,
    plataforma: String,
    version: String,
    archivo: String,
    bytes: u64,
    sha256: String,
    /// Segundos desde 1970 (UTC).
    fecha: u64,
    nota: String,
}

#[derive(Deserialize)]
struct Publicar {
    plataforma: String,
    archivo: String,
    #[serde(default)]
    version: String,
    #[serde(default)]
    nota: String,
}

type Resultado<T> = Result<T, (StatusCode, String)>;

fn error(c: StatusCode, m: impl Into<String>) -> (StatusCode, String) {
    (c, m.into())
}

fn interno<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    error(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

#[tokio::main]
async fn main() {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let datos = std::env::var("GRAFO_DATOS")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(&home).join(".local/share/grafo-publicador"));
    let fuente = std::env::var("GRAFO_FUENTE").map(PathBuf::from).unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("carpeta padre")
            .to_path_buf()
    });
    let escucha = std::env::var("GRAFO_ESCUCHA").unwrap_or_else(|_| "127.0.0.1:8097".into());

    std::fs::create_dir_all(datos.join("versiones")).expect("crear carpeta de datos");
    let token = leer_o_crear_token(&datos.join("token"));
    let estado = Arc::new(Estado { datos: datos.clone(), fuente: fuente.clone(), token });

    let app = Router::new()
        .route("/", get(|| async { Html(PAGINA) }))
        .route("/api/versiones", get(listar))
        .route("/descargar/{id}", get(descargar))
        .route("/ultima/{plataforma}", get(ultima))
        .route("/versiones/{id}", delete(borrar))
        .route(
            "/publicar",
            post(publicar).layer(DefaultBodyLimit::max(MAX_SUBIDA)),
        )
        .route("/fuente.tar.gz", get(fuente_tar))
        .route("/instalar.sh", get(|h: HeaderMap| async move { script(INSTALAR, &h) }))
        .route("/descargar.sh", get(|h: HeaderMap| async move { script(DESCARGAR, &h) }))
        .with_state(estado);

    let oyente = tokio::net::TcpListener::bind(&escucha)
        .await
        .unwrap_or_else(|e| panic!("no se pudo escuchar en {escucha}: {e}"));
    println!("grafo-publicador en http://{escucha}");
    println!("  datos:  {}", datos.display());
    println!("  fuente: {}", fuente.display());
    println!("  token:  {}", datos.join("token").display());
    axum::serve(oyente, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await
        .expect("servidor");
}

fn leer_o_crear_token(ruta: &Path) -> String {
    if let Ok(t) = std::fs::read_to_string(ruta) {
        let t = t.trim().to_string();
        if !t.is_empty() {
            return t;
        }
    }
    let mut bytes = [0u8; 16];
    use std::io::Read;
    std::fs::File::open("/dev/urandom")
        .and_then(|mut f| f.read_exact(&mut bytes))
        .expect("leer /dev/urandom");
    let t = hex::encode(bytes);
    std::fs::write(ruta, format!("{t}\n")).expect("guardar token");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(ruta, std::fs::Permissions::from_mode(0o600));
    }
    t
}

fn autorizar(e: &Estado, h: &HeaderMap) -> Resultado<()> {
    let dado = h.get("x-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if dado.as_bytes() == e.token.as_bytes() {
        Ok(())
    } else {
        Err(error(StatusCode::UNAUTHORIZED, "token incorrecto"))
    }
}

/// Solo letras, números, punto, guion y guion bajo (evita rutas como `../`).
fn limpio(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 120
        && !s.starts_with('.')
        && s.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
}

async fn leer_versiones(e: &Estado) -> Vec<Version> {
    let mut out = Vec::new();
    let Ok(mut it) = tokio::fs::read_dir(e.datos.join("versiones")).await else { return out };
    while let Ok(Some(d)) = it.next_entry().await {
        if let Ok(txt) = tokio::fs::read_to_string(d.path().join("meta.json")).await {
            if let Ok(v) = serde_json::from_str::<Version>(&txt) {
                out.push(v);
            }
        }
    }
    out.sort_by(|a, b| b.fecha.cmp(&a.fecha).then(b.id.cmp(&a.id)));
    out
}

async fn listar(State(e): State<Compartido>) -> Json<Vec<Version>> {
    Json(leer_versiones(&e).await)
}

async fn enviar_archivo(e: &Estado, v: &Version) -> Resultado<Response> {
    let ruta = e.datos.join("versiones").join(&v.id).join(&v.archivo);
    let f = tokio::fs::File::open(&ruta).await.map_err(interno)?;
    let mut r = Body::from_stream(ReaderStream::new(f)).into_response();
    let h = r.headers_mut();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    h.insert(header::CONTENT_LENGTH, HeaderValue::from(v.bytes));
    if let Ok(cd) = HeaderValue::from_str(&format!("attachment; filename=\"{}\"", v.archivo)) {
        h.insert(header::CONTENT_DISPOSITION, cd);
    }
    if let Ok(s) = HeaderValue::from_str(&v.sha256) {
        h.insert("x-sha256", s);
    }
    Ok(r)
}

async fn descargar(State(e): State<Compartido>, Ruta(id): Ruta<String>) -> Resultado<Response> {
    if !limpio(&id) {
        return Err(error(StatusCode::BAD_REQUEST, "id no válido"));
    }
    let v = leer_versiones(&e)
        .await
        .into_iter()
        .find(|v| v.id == id)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no existe esa versión"))?;
    enviar_archivo(&e, &v).await
}

async fn ultima(State(e): State<Compartido>, Ruta(p): Ruta<String>) -> Resultado<Response> {
    let v = leer_versiones(&e)
        .await
        .into_iter()
        .find(|v| v.plataforma == p)
        .ok_or_else(|| error(StatusCode::NOT_FOUND, format!("no hay versiones para {p}")))?;
    enviar_archivo(&e, &v).await
}

async fn borrar(
    State(e): State<Compartido>,
    h: HeaderMap,
    Ruta(id): Ruta<String>,
) -> Resultado<StatusCode> {
    autorizar(&e, &h)?;
    if !limpio(&id) {
        return Err(error(StatusCode::BAD_REQUEST, "id no válido"));
    }
    let dir = e.datos.join("versiones").join(&id);
    if !dir.is_dir() {
        return Err(error(StatusCode::NOT_FOUND, "no existe esa versión"));
    }
    tokio::fs::remove_dir_all(dir).await.map_err(interno)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Recibe el archivo como cuerpo crudo (`curl --data-binary @archivo`), con los datos en la URL.
async fn publicar(
    State(e): State<Compartido>,
    h: HeaderMap,
    Query(q): Query<Publicar>,
    cuerpo: Body,
) -> Resultado<(StatusCode, Json<Version>)> {
    autorizar(&e, &h)?;
    if !limpio(&q.plataforma) || !limpio(&q.archivo) {
        return Err(error(
            StatusCode::BAD_REQUEST,
            "plataforma y archivo solo admiten letras, números, '.', '-' y '_'",
        ));
    }
    let ahora = SystemTime::now().duration_since(UNIX_EPOCH).map_err(interno)?;
    let id = format!("{}-{}", ahora.as_millis(), q.plataforma);
    let dir = e.datos.join("versiones").join(&id);
    let tmp = e.datos.join("versiones").join(format!(".subiendo-{id}"));
    tokio::fs::create_dir_all(&tmp).await.map_err(interno)?;

    let resultado = async {
        let mut f = tokio::fs::File::create(tmp.join(&q.archivo)).await.map_err(interno)?;
        let mut hash = Sha256::new();
        let mut bytes = 0u64;
        let mut flujo = cuerpo.into_data_stream();
        while let Some(trozo) = flujo.next().await {
            let trozo = trozo.map_err(|e| error(StatusCode::BAD_REQUEST, e.to_string()))?;
            hash.update(&trozo);
            bytes += trozo.len() as u64;
            f.write_all(&trozo).await.map_err(interno)?;
        }
        f.flush().await.map_err(interno)?;
        f.sync_all().await.map_err(interno)?;
        if bytes == 0 {
            return Err(error(StatusCode::BAD_REQUEST, "el archivo está vacío"));
        }
        let v = Version {
            id: id.clone(),
            plataforma: q.plataforma.clone(),
            version: q.version.chars().take(60).collect(),
            archivo: q.archivo.clone(),
            bytes,
            sha256: hex::encode(hash.finalize()),
            fecha: ahora.as_secs(),
            nota: q.nota.chars().take(500).collect(),
        };
        let meta = serde_json::to_string_pretty(&v).map_err(interno)?;
        tokio::fs::write(tmp.join("meta.json"), meta).await.map_err(interno)?;
        tokio::fs::rename(&tmp, &dir).await.map_err(interno)?;
        Ok(v)
    }
    .await;

    match resultado {
        Ok(v) => {
            println!("publicada {} ({}, {} bytes)", v.id, v.archivo, v.bytes);
            Ok((StatusCode::CREATED, Json(v)))
        }
        Err(err) => {
            let _ = tokio::fs::remove_dir_all(&tmp).await;
            Err(err)
        }
    }
}

/// Código fuente de grafo (sin `target/` ni el publicador) para compilarlo en la Mac.
async fn fuente_tar(State(e): State<Compartido>) -> Resultado<Response> {
    let salida = tokio::process::Command::new("tar")
        .arg("-C")
        .arg(&e.fuente)
        .args([
            "--exclude=./target",
            "--exclude=./publicador",
            "--exclude=./.git",
            "-czf",
            "-",
            "Cargo.toml",
            "Cargo.lock",
            "src",
        ])
        .output()
        .await
        .map_err(interno)?;
    if !salida.status.success() {
        return Err(interno(String::from_utf8_lossy(&salida.stderr)));
    }
    Ok((
        [
            (header::CONTENT_TYPE, "application/gzip"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"grafo-fuente.tar.gz\""),
        ],
        salida.stdout,
    )
        .into_response())
}

/// Los scripts llevan la dirección del servidor tal como la usó quien los pidió.
fn script(plantilla: &str, h: &HeaderMap) -> Response {
    let host = h
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .filter(|s| s.chars().all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c)))
        .unwrap_or("127.0.0.1:8097");
    (
        [(header::CONTENT_TYPE, "text/x-shellscript; charset=utf-8")],
        plantilla.replace("{{SERVIDOR}}", &format!("http://{host}")),
    )
        .into_response()
}

#[cfg(test)]
mod pruebas {
    use super::limpio;

    #[test]
    fn nombres() {
        assert!(limpio("grafo-macos-arm64.zip"));
        assert!(!limpio("../token"));
        assert!(!limpio(".oculto"));
        assert!(!limpio("a/b"));
        assert!(!limpio(""));
    }
}
