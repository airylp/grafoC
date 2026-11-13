//! Lectura de `~/.claude`: carpetas, proyectos, sesiones, archivos modificados y pendientes.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

use chrono::{DateTime, Local};
use serde_json::Value;

/// Máximo de archivos modificados que se dibujan por proyecto (la lista lateral los tiene todos).
const ARCHIVOS_POR_PROYECTO: usize = 30;
/// Máximo de pendientes (TODO + git) que se dibujan por proyecto.
const PENDIENTES_POR_PROYECTO: usize = 25;
/// Límite de archivos revisados por proyecto al buscar TODO/FIXME.
const ARCHIVOS_ESCANEADOS: usize = 4000;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Tipo {
    Raiz,
    Carpeta,
    Proyecto,
    Sesion,
    Archivo,
    Pendiente,
}

impl Tipo {
    pub const TODOS: [Tipo; 6] = [
        Tipo::Raiz,
        Tipo::Carpeta,
        Tipo::Proyecto,
        Tipo::Sesion,
        Tipo::Archivo,
        Tipo::Pendiente,
    ];

    pub fn nombre(self) -> &'static str {
        match self {
            Tipo::Raiz => "Raíz",
            Tipo::Carpeta => "Carpetas de Claude",
            Tipo::Proyecto => "Proyectos",
            Tipo::Sesion => "Sesiones",
            Tipo::Archivo => "Archivos modificados",
            Tipo::Pendiente => "Pendientes",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Nodo {
    pub tipo: Tipo,
    pub etiqueta: String,
    pub detalle: Vec<(String, String)>,
    pub ruta: Option<PathBuf>,
    pub fecha: Option<DateTime<Local>>,
    /// Tamaño relativo del nodo (1.0 = normal).
    pub peso: f32,
    /// Sesión en curso (proceso vivo).
    pub activa: bool,
    /// Proyecto al que pertenece (para la lista lateral).
    pub proyecto: Option<usize>,
}

#[derive(Default)]
pub struct Datos {
    pub nodos: Vec<Nodo>,
    pub aristas: Vec<(usize, usize)>,
    /// Ids de nodos Archivo ordenados del más reciente al más antiguo.
    pub recientes: Vec<usize>,
    /// Ids de nodos Pendiente.
    pub pendientes: Vec<usize>,
}

pub enum Mensaje {
    Progreso(String),
    Listo(Datos),
}

impl Datos {
    fn agregar(&mut self, nodo: Nodo, padre: Option<usize>) -> usize {
        let id = self.nodos.len();
        self.nodos.push(nodo);
        if let Some(p) = padre {
            self.aristas.push((p, id));
        }
        id
    }
}

fn nodo(tipo: Tipo, etiqueta: impl Into<String>) -> Nodo {
    Nodo {
        tipo,
        etiqueta: etiqueta.into(),
        detalle: Vec::new(),
        ruta: None,
        fecha: None,
        peso: 1.0,
        activa: false,
        proyecto: None,
    }
}

pub fn dir_claude() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            let home = std::env::var_os("HOME").unwrap_or_default();
            PathBuf::from(home).join(".claude")
        })
}

/// Ruta con `~` en lugar de la carpeta personal.
pub fn ruta_corta(p: &Path) -> String {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    match home.as_deref().and_then(|h| p.strip_prefix(h).ok()) {
        Some(resto) if !resto.as_os_str().is_empty() => format!("~/{}", resto.display()),
        _ => p.display().to_string(),
    }
}

pub fn cargar(raiz: PathBuf, tx: Sender<Mensaje>) {
    let mut d = Datos::default();
    let _ = tx.send(Mensaje::Progreso("Leyendo carpetas de Claude…".into()));

    let mut n = nodo(Tipo::Raiz, &ruta_corta(&raiz));
    n.ruta = Some(raiz.clone());
    n.peso = 2.2;
    let id_raiz = d.agregar(n, None);

    // --- Carpetas de primer nivel ---
    let mut entradas: Vec<_> = fs::read_dir(&raiz)
        .map(|it| it.flatten().collect())
        .unwrap_or_default();
    entradas.sort_by_key(|e| e.file_name());
    let mut id_projects = None;
    for e in entradas {
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_dir() {
            continue;
        }
        let nombre = e.file_name().to_string_lossy().into_owned();
        let _ = tx.send(Mensaje::Progreso(format!("Midiendo {nombre}…")));
        let (bytes, archivos, ultima) = medir(&e.path());
        let mut n = nodo(Tipo::Carpeta, nombre.clone());
        n.ruta = Some(e.path());
        n.fecha = ultima;
        n.peso = 1.0 + ((bytes as f32).max(1.0).log10() / 10.0).min(0.9);
        n.detalle.push(("Tamaño".into(), formato_bytes(bytes)));
        n.detalle.push(("Archivos".into(), archivos.to_string()));
        let id = d.agregar(n, Some(id_raiz));
        if nombre == "projects" {
            id_projects = Some(id);
        }
    }
    let id_projects = id_projects.unwrap_or(id_raiz);

    // --- Sesiones vivas ---
    let activas = sesiones_activas(&raiz.join("sessions"));

    // --- Proyectos y sesiones ---
    let mut todos_archivos: Vec<(DateTime<Local>, usize)> = Vec::new();
    let mut proyectos: Vec<_> = fs::read_dir(raiz.join("projects"))
        .map(|it| it.flatten().filter(|e| e.path().is_dir()).collect())
        .unwrap_or_default();
    proyectos.sort_by_key(|e| e.file_name());

    for p in proyectos {
        let dir = p.path();
        let clave = p.file_name().to_string_lossy().into_owned();
        let _ = tx.send(Mensaje::Progreso(format!("Leyendo sesiones de {clave}…")));

        let mut sesiones: Vec<Sesion> = fs::read_dir(&dir)
            .map(|it| {
                it.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "jsonl"))
                    .filter_map(|p| leer_sesion(&p))
                    .collect()
            })
            .unwrap_or_default();
        if sesiones.is_empty() {
            continue;
        }
        sesiones.sort_by(|a, b| b.ultima.cmp(&a.ultima));

        let cwd = sesiones.iter().find_map(|s| s.cwd.clone());
        let nombre = cwd
            .as_ref()
            .and_then(|c| Path::new(c).file_name())
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| clave.clone());
        let costo: f64 = sesiones.iter().map(|s| s.costo).sum();
        let memorias = fs::read_dir(dir.join("memory"))
            .map(|it| {
                it.flatten()
                    .filter(|e| e.file_name() != "MEMORY.md")
                    .count()
            })
            .unwrap_or(0);

        let mut n = nodo(Tipo::Proyecto, nombre);
        n.ruta = cwd.as_ref().map(PathBuf::from).or(Some(dir.clone()));
        n.fecha = sesiones.first().and_then(|s| s.ultima);
        n.peso = 1.4 + (sesiones.len() as f32).sqrt() * 0.12;
        n.detalle.push(("Directorio".into(), cwd.clone().unwrap_or_default()));
        n.detalle.push(("Sesiones".into(), sesiones.len().to_string()));
        n.detalle.push(("Costo total".into(), format!("${costo:.2} USD")));
        n.detalle.push(("Memorias".into(), memorias.to_string()));
        let id_proy = d.agregar(n, Some(id_projects));
        d.nodos[id_proy].proyecto = Some(id_proy);

        // Archivos del proyecto: ruta -> (última fecha, sesiones que lo tocaron, ediciones)
        let mut archivos: HashMap<String, (DateTime<Local>, Vec<usize>, usize)> = HashMap::new();
        for s in &sesiones {
            let mut n = nodo(Tipo::Sesion, s.titulo());
            n.ruta = Some(s.archivo.clone());
            n.fecha = s.ultima;
            n.activa = activas.contains(&s.id);
            n.peso = 0.8 + ((s.mensajes as f32).max(1.0).log10() * 0.25).min(0.9);
            n.proyecto = Some(id_proy);
            n.detalle.push(("Id".into(), s.id.clone()));
            if let Some(i) = s.inicio {
                n.detalle.push(("Inicio".into(), i.format("%Y-%m-%d %H:%M").to_string()));
            }
            if let Some(u) = s.ultima {
                n.detalle.push(("Última actividad".into(), u.format("%Y-%m-%d %H:%M").to_string()));
            }
            n.detalle.push(("Mensajes".into(), s.mensajes.to_string()));
            n.detalle.push(("Costo".into(), format!("${:.2} USD", s.costo)));
            n.detalle.push(("Archivos editados".into(), s.ediciones.len().to_string()));
            if let Some(p) = &s.primer_prompt {
                n.detalle.push(("Primer prompt".into(), recortar(p, 300)));
            }
            if let Some(p) = &s.ultimo_prompt {
                n.detalle.push(("Último prompt".into(), recortar(p, 300)));
            }
            if n.activa {
                n.detalle.push(("Estado".into(), "En curso".into()));
            }
            let id_ses = d.agregar(n, Some(id_proy));
            for (ruta, (fecha, veces)) in &s.ediciones {
                let e = archivos
                    .entry(ruta.clone())
                    .or_insert((*fecha, Vec::new(), 0));
                if *fecha > e.0 {
                    e.0 = *fecha;
                }
                e.1.push(id_ses);
                e.2 += veces;
            }
        }

        let mut lista: Vec<_> = archivos.into_iter().collect();
        lista.sort_by(|a, b| b.1 .0.cmp(&a.1 .0));
        for (ruta, (fecha, sesiones_f, ediciones)) in lista.into_iter().take(ARCHIVOS_POR_PROYECTO) {
            let p = PathBuf::from(&ruta);
            let base = p
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| ruta.clone());
            let mut n = nodo(Tipo::Archivo, base);
            n.fecha = Some(fecha);
            n.peso = 0.55 + ((ediciones as f32).sqrt() * 0.08).min(0.6);
            n.proyecto = Some(id_proy);
            n.detalle.push(("Ruta".into(), ruta.clone()));
            n.detalle.push(("Ediciones".into(), ediciones.to_string()));
            n.detalle.push(("Sesiones".into(), sesiones_f.len().to_string()));
            if !p.exists() {
                n.detalle.push(("Estado".into(), "Ya no existe".into()));
            }
            n.ruta = Some(p);
            // El padre es la sesión más reciente; el resto son aristas extra.
            let id = d.agregar(n, sesiones_f.first().copied());
            for &s in sesiones_f.iter().skip(1) {
                d.aristas.push((s, id));
            }
            todos_archivos.push((fecha, id));
        }

        // --- Pendientes ---
        if let Some(c) = &cwd {
            let c = Path::new(c);
            if c.is_dir() {
                let _ = tx.send(Mensaje::Progreso(format!("Buscando pendientes en {}…", c.display())));
                for pend in buscar_pendientes(c).into_iter().take(PENDIENTES_POR_PROYECTO) {
                    let mut n = nodo(Tipo::Pendiente, pend.etiqueta);
                    n.ruta = Some(pend.ruta);
                    n.fecha = pend.fecha;
                    n.peso = 0.6;
                    n.proyecto = Some(id_proy);
                    n.detalle = pend.detalle;
                    let id = d.agregar(n, Some(id_proy));
                    d.pendientes.push(id);
                }
            }
        }
    }

    todos_archivos.sort_by(|a, b| b.0.cmp(&a.0));
    d.recientes = todos_archivos.into_iter().map(|(_, id)| id).collect();
    let _ = tx.send(Mensaje::Listo(d));
}

// ---------------------------------------------------------------------------
// Sesiones

struct Sesion {
    id: String,
    archivo: PathBuf,
    cwd: Option<String>,
    inicio: Option<DateTime<Local>>,
    ultima: Option<DateTime<Local>>,
    titulo_ia: Option<String>,
    titulo_propio: Option<String>,
    primer_prompt: Option<String>,
    ultimo_prompt: Option<String>,
    mensajes: usize,
    costo: f64,
    /// ruta -> (última edición, número de ediciones)
    ediciones: HashMap<String, (DateTime<Local>, usize)>,
}

impl Sesion {
    fn titulo(&self) -> String {
        let t = self
            .titulo_propio
            .clone()
            .or_else(|| self.titulo_ia.clone())
            .or_else(|| self.primer_prompt.clone())
            .or_else(|| self.ultimo_prompt.clone())
            .unwrap_or_else(|| self.id.chars().take(8).collect());
        recortar(&t, 48)
    }
}

fn leer_sesion(ruta: &Path) -> Option<Sesion> {
    let f = fs::File::open(ruta).ok()?;
    let id = ruta.file_stem()?.to_string_lossy().into_owned();
    let mut s = Sesion {
        id,
        archivo: ruta.to_path_buf(),
        cwd: None,
        inicio: None,
        ultima: None,
        titulo_ia: None,
        titulo_propio: None,
        primer_prompt: None,
        ultimo_prompt: None,
        mensajes: 0,
        costo: 0.0,
        ediciones: HashMap::new(),
    };
    for linea in BufReader::new(f).lines() {
        let Ok(linea) = linea else { continue };
        // Las copias de peticiones a la API son enormes y no aportan nada aquí.
        if linea.starts_with("{\"type\":\"api-request") {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(&linea) else { continue };
        let ts = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&Local));
        let tipo = v.get("type").and_then(Value::as_str).unwrap_or("");
        match tipo {
            "user" | "assistant" => {
                if let Some(t) = ts {
                    if s.inicio.is_none() {
                        s.inicio = Some(t);
                    }
                    s.ultima = Some(t);
                }
                if s.cwd.is_none() {
                    s.cwd = v.get("cwd").and_then(Value::as_str).map(String::from);
                }
                let contenido = v.pointer("/message/content");
                if tipo == "user" {
                    if let Some(texto) = texto_de_usuario(contenido) {
                        s.mensajes += 1;
                        if s.primer_prompt.is_none() {
                            s.primer_prompt = Some(texto);
                        }
                    }
                } else {
                    s.mensajes += 1;
                    if let Some(Value::Array(bloques)) = contenido {
                        for b in bloques {
                            if b.get("type").and_then(Value::as_str) != Some("tool_use") {
                                continue;
                            }
                            let nombre = b.get("name").and_then(Value::as_str).unwrap_or("");
                            if !matches!(nombre, "Edit" | "Write" | "MultiEdit" | "NotebookEdit") {
                                continue;
                            }
                            let ruta = b
                                .pointer("/input/file_path")
                                .or_else(|| b.pointer("/input/notebook_path"))
                                .and_then(Value::as_str);
                            if let (Some(r), Some(t)) = (ruta, ts) {
                                let e = s.ediciones.entry(r.to_string()).or_insert((t, 0));
                                e.0 = e.0.max(t);
                                e.1 += 1;
                            }
                        }
                    }
                }
            }
            "last-prompt" => {
                s.ultimo_prompt = v.get("lastPrompt").and_then(Value::as_str).map(String::from);
            }
            "ai-title" => {
                s.titulo_ia = v.get("aiTitle").and_then(Value::as_str).map(String::from);
            }
            "custom-title" => {
                s.titulo_propio = v.get("customTitle").and_then(Value::as_str).map(String::from);
            }
            "cost-state" => {
                if let Some(c) = v.get("totalCostUSD").and_then(Value::as_f64) {
                    s.costo = c;
                }
            }
            _ => {}
        }
    }
    if s.mensajes == 0 && s.ultima.is_none() {
        return None;
    }
    Some(s)
}

/// Texto escrito por la persona (ignora resultados de herramientas y mensajes del sistema).
fn texto_de_usuario(c: Option<&Value>) -> Option<String> {
    let t = match c? {
        Value::String(s) => s.clone(),
        Value::Array(bloques) => bloques
            .iter()
            .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .find(|t| !t.trim_start().starts_with('<'))?
            .to_string(),
        _ => return None,
    };
    let t = t.trim();
    if t.is_empty() || t.starts_with('<') {
        return None;
    }
    Some(t.to_string())
}

fn sesiones_activas(dir: &Path) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(it) = fs::read_dir(dir) else { return set };
    for e in it.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") {
            continue;
        }
        let Ok(txt) = fs::read_to_string(&p) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&txt) else { continue };
        let vivo = v
            .get("pid")
            .and_then(Value::as_u64)
            .is_some_and(proceso_vivo);
        if let (true, Some(id)) = (vivo, v.get("sessionId").and_then(Value::as_str)) {
            set.insert(id.to_string());
        }
    }
    set
}

/// ¿Sigue vivo el proceso? En Linux basta con /proc; en macOS se pregunta a `kill -0`.
fn proceso_vivo(pid: u64) -> bool {
    if cfg!(target_os = "linux") {
        return Path::new(&format!("/proc/{pid}")).exists();
    }
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

// ---------------------------------------------------------------------------
// Pendientes

struct Pendiente {
    etiqueta: String,
    ruta: PathBuf,
    fecha: Option<DateTime<Local>>,
    detalle: Vec<(String, String)>,
}

const MARCAS: [&str; 3] = ["TODO", "FIXME", "PENDIENTE"];
const EXTENSIONES: [&str; 24] = [
    "rs", "py", "js", "ts", "tsx", "jsx", "go", "c", "h", "cpp", "hpp", "java", "kt", "sh", "html",
    "css", "vue", "svelte", "md", "toml", "yaml", "yml", "rb", "php",
];
const DIRS_IGNORADOS: [&str; 12] = [
    ".git", "target", "node_modules", ".venv", "venv", "__pycache__", "dist", "build", ".next",
    ".cache", "vendor", ".idea",
];

fn buscar_pendientes(cwd: &Path) -> Vec<Pendiente> {
    let mut out = Vec::new();

    // Cambios sin commit (si es un repositorio git).
    if let Ok(o) = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .output()
    {
        if o.status.success() {
            for l in String::from_utf8_lossy(&o.stdout).lines() {
                if l.len() < 4 {
                    continue;
                }
                let (estado, ruta) = l.split_at(3);
                let abs = cwd.join(ruta.trim());
                let desc = match estado.trim() {
                    "??" => "sin seguimiento",
                    "M" | "MM" | "AM" => "modificado",
                    "A" => "agregado",
                    "D" => "eliminado",
                    "R" => "renombrado",
                    _ => "cambiado",
                };
                out.push(Pendiente {
                    etiqueta: format!("git: {}", ruta.trim()),
                    fecha: fecha_mod(&abs),
                    detalle: vec![
                        ("Tipo".into(), "Cambio sin commit".into()),
                        ("Estado".into(), desc.into()),
                        ("Ruta".into(), abs.display().to_string()),
                    ],
                    ruta: abs,
                });
            }
        }
    }

    // Marcas TODO / FIXME / PENDIENTE en el código.
    let mut pila = vec![cwd.to_path_buf()];
    let mut vistos = 0usize;
    let mut marcas = Vec::new();
    while let Some(dir) = pila.pop() {
        let Ok(it) = fs::read_dir(&dir) else { continue };
        for e in it.flatten() {
            let p = e.path();
            let Ok(ft) = e.file_type() else { continue };
            let nombre = e.file_name().to_string_lossy().into_owned();
            if ft.is_dir() {
                if !DIRS_IGNORADOS.contains(&nombre.as_str()) && !nombre.starts_with('.') {
                    pila.push(p);
                }
                continue;
            }
            if !ft.is_file() {
                continue;
            }
            let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase());
            if !ext.is_some_and(|x| EXTENSIONES.contains(&x.as_str())) {
                continue;
            }
            if e.metadata().map(|m| m.len() > 512 * 1024).unwrap_or(true) {
                continue;
            }
            vistos += 1;
            if vistos > ARCHIVOS_ESCANEADOS {
                break;
            }
            let Ok(txt) = fs::read_to_string(&p) else { continue };
            for (i, l) in txt.lines().enumerate() {
                let Some(m) = MARCAS.iter().find(|m| contiene_marca(l, m)) else { continue };
                let texto = l.trim();
                // Descarta menciones sin contenido (p. ej. la palabra dentro de un string).
                if texto_tras_marca(texto, m).chars().filter(|c| c.is_alphanumeric()).count() < 3 {
                    continue;
                }
                marcas.push(Pendiente {
                    etiqueta: format!("{m}: {}", recortar(texto_tras_marca(texto, m), 40)),
                    fecha: fecha_mod(&p),
                    detalle: vec![
                        ("Tipo".into(), format!("Marca {m}")),
                        ("Línea".into(), recortar(texto, 300)),
                        ("Ubicación".into(), format!("{}:{}", p.display(), i + 1)),
                    ],
                    ruta: p.clone(),
                });
            }
        }
        if vistos > ARCHIVOS_ESCANEADOS {
            break;
        }
    }
    marcas.sort_by(|a, b| b.fecha.cmp(&a.fecha));
    out.extend(marcas);
    out
}

/// La marca debe aparecer como palabra completa en mayúsculas (evita "todos", "TODOS"...).
fn contiene_marca(linea: &str, marca: &str) -> bool {
    let b = linea.as_bytes();
    let mut desde = 0;
    while let Some(pos) = linea[desde..].find(marca) {
        let i = desde + pos;
        let fin = i + marca.len();
        let antes_ok = i == 0 || !b[i - 1].is_ascii_alphanumeric();
        let despues_ok = fin >= b.len() || !(b[fin].is_ascii_alphanumeric() || b[fin] == b'_');
        if antes_ok && despues_ok {
            return true;
        }
        desde = fin;
    }
    false
}

fn texto_tras_marca<'a>(linea: &'a str, marca: &str) -> &'a str {
    linea
        .find(marca)
        .map(|i| linea[i + marca.len()..].trim_start_matches([':', ' ', '(', ')', '-']))
        .filter(|t| !t.is_empty())
        .unwrap_or(linea)
}

// ---------------------------------------------------------------------------
// Utilidades

fn fecha_mod(p: &Path) -> Option<DateTime<Local>> {
    let t = fs::metadata(p).ok()?.modified().ok()?;
    Some(DateTime::<Local>::from(t))
}

/// Tamaño total, número de archivos y fecha de la última modificación.
fn medir(dir: &Path) -> (u64, u64, Option<DateTime<Local>>) {
    let mut bytes = 0;
    let mut n = 0;
    let mut ultima: Option<std::time::SystemTime> = None;
    let mut pila = vec![dir.to_path_buf()];
    while let Some(d) = pila.pop() {
        let Ok(it) = fs::read_dir(&d) else { continue };
        for e in it.flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() {
                pila.push(e.path());
            } else {
                bytes += m.len();
                n += 1;
                if let Ok(t) = m.modified() {
                    ultima = Some(ultima.map_or(t, |u| u.max(t)));
                }
            }
        }
    }
    (bytes, n, ultima.map(DateTime::<Local>::from))
}

pub fn formato_bytes(b: u64) -> String {
    let u = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < u.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{b} B") } else { format!("{v:.1} {}", u[i]) }
}

pub fn recortar(s: &str, max: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= max {
        s
    } else {
        let mut t: String = s.chars().take(max).collect();
        t.push('…');
        t
    }
}

pub fn hace(f: DateTime<Local>) -> String {
    let s = (Local::now() - f).num_seconds().max(0);
    match s {
        0..60 => "hace un momento".into(),
        60..3600 => format!("hace {} min", s / 60),
        3600..86400 => format!("hace {} h", s / 3600),
        86400..2_592_000 => format!("hace {} d", s / 86400),
        _ => f.format("%Y-%m-%d").to_string(),
    }
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn marca_palabra_completa() {
        assert!(contiene_marca("// TODO: algo", "TODO"));
        assert!(contiene_marca("x = 1 # FIXME", "FIXME"));
        assert!(!contiene_marca("TODOS los casos", "TODO"));
        assert!(!contiene_marca("MY_TODO_LIST", "TODO"));
        assert_eq!(texto_tras_marca("// TODO: revisar", "TODO"), "revisar");
    }

    #[test]
    fn bytes() {
        assert_eq!(formato_bytes(512), "512 B");
        assert_eq!(formato_bytes(2048), "2.0 KB");
    }
}
