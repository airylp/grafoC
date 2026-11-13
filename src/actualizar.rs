//! Buscar e instalar actualizaciones desde GitHub (con el mismo install.sh) y
//! reiniciar el programa. Todo corre en hilos; la interfaz solo lee el estado.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;

use eframe::egui;

pub const REPO: &str = "airylp/grafoC";
pub const COMANDO_INSTALAR: &str = "curl -fsSL https://raw.githubusercontent.com/airylp/grafoC/main/install.sh | sh";

/// Commit con que se compiló (lo pone install.sh), si se conoce.
pub fn commit() -> Option<&'static str> {
    option_env!("GRAFO_COMMIT").filter(|c| !c.is_empty())
}

/// «0.1.0 (abc1234)».
pub fn version() -> String {
    match commit() {
        Some(c) => format!("{} ({c})", env!("CARGO_PKG_VERSION")),
        None => env!("CARGO_PKG_VERSION").to_string(),
    }
}

#[derive(Clone, PartialEq)]
pub enum Estado {
    Inactivo,
    Buscando,
    AlDia,
    /// Último commit publicado.
    Nueva(String),
    Instalando,
    Instalada,
    Error(String),
}

pub struct Actualizador {
    estado: Arc<Mutex<Estado>>,
    /// Salida del instalador (últimas líneas).
    registro: Arc<Mutex<Vec<String>>>,
}

impl Default for Actualizador {
    fn default() -> Self {
        Actualizador { estado: Arc::new(Mutex::new(Estado::Inactivo)), registro: Arc::new(Mutex::new(Vec::new())) }
    }
}

/// Quita los códigos de color ANSI (`\x1b[...m`).
fn sin_colores(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for c in s.chars() {
        match (esc, c) {
            (false, '\x1b') => esc = true,
            (true, c) if c.is_ascii_alphabetic() => esc = false,
            (true, _) => {}
            (false, c) => out.push(c),
        }
    }
    out
}

impl Actualizador {
    pub fn estado(&self) -> Estado {
        self.estado.lock().map(|e| e.clone()).unwrap_or(Estado::Inactivo)
    }

    pub fn ultima_linea(&self) -> String {
        self.registro.lock().ok().and_then(|r| r.last().cloned()).unwrap_or_default()
    }

    pub fn registro(&self) -> Vec<String> {
        self.registro.lock().map(|r| r.clone()).unwrap_or_default()
    }

    pub fn ocupado(&self) -> bool {
        matches!(self.estado(), Estado::Buscando | Estado::Instalando)
    }

    fn poner(estado: &Arc<Mutex<Estado>>, e: Estado, ctx: &egui::Context) {
        if let Ok(mut g) = estado.lock() {
            *g = e;
        }
        ctx.request_repaint();
    }

    /// Pregunta a GitHub el último commit de `main` y lo compara con el propio.
    pub fn buscar(&self, ctx: &egui::Context) {
        if self.ocupado() {
            return;
        }
        let estado = self.estado.clone();
        let ctx = ctx.clone();
        Self::poner(&estado, Estado::Buscando, &ctx);
        thread::spawn(move || {
            let r = Command::new("curl")
                .args(["-fsSL", "--max-time", "20", "-H", "Accept: application/vnd.github.sha"])
                .arg(format!("https://api.github.com/repos/{REPO}/commits/main"))
                .stdin(Stdio::null())
                .output();
            let e = match r {
                Ok(o) if o.status.success() => {
                    let sha: String = String::from_utf8_lossy(&o.stdout).trim().chars().take(7).collect();
                    if sha.len() < 7 {
                        Estado::Error("respuesta inesperada de GitHub".into())
                    } else if commit() == Some(sha.as_str()) {
                        Estado::AlDia
                    } else {
                        Estado::Nueva(sha)
                    }
                }
                Ok(o) => Estado::Error(format!("no se pudo consultar GitHub ({})", sin_colores(String::from_utf8_lossy(&o.stderr).trim()))),
                Err(e) => Estado::Error(format!("no se pudo ejecutar curl: {e}")),
            };
            Self::poner(&estado, e, &ctx);
        });
    }

    /// Descarga, compila e instala la última versión con install.sh (sin preguntas).
    pub fn instalar(&self, ctx: &egui::Context, idioma: Option<&str>) {
        if self.ocupado() {
            return;
        }
        let estado = self.estado.clone();
        let registro = self.registro.clone();
        let ctx = ctx.clone();
        if let Ok(mut r) = registro.lock() {
            r.clear();
        }
        Self::poner(&estado, Estado::Instalando, &ctx);
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(format!("curl -fsSL https://raw.githubusercontent.com/{REPO}/main/install.sh | sh -s -- instalar"))
            .env("GRAFO_NO_CERRAR", "1")
            .env("GRAFO_IDIOMA", idioma.unwrap_or("es"))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // En macOS se reemplaza la app donde esté (Aplicaciones o ~/Applications).
        if let Some(dir) = app_mac().and_then(|a| a.parent().map(PathBuf::from)) {
            cmd.env("GRAFO_DESTINO", dir);
        }
        thread::spawn(move || {
            let mut hijo = match cmd.spawn() {
                Ok(h) => h,
                Err(e) => return Self::poner(&estado, Estado::Error(format!("no se pudo iniciar: {e}")), &ctx),
            };
            let leer = |r: Box<dyn std::io::Read + Send>| {
                let registro = registro.clone();
                let ctx = ctx.clone();
                thread::spawn(move || {
                    for l in BufReader::new(r).lines().map_while(Result::ok) {
                        let l = sin_colores(&l);
                        if l.trim().is_empty() {
                            continue;
                        }
                        if let Ok(mut reg) = registro.lock() {
                            reg.push(l);
                            let n = reg.len();
                            if n > 300 {
                                reg.drain(..n - 300);
                            }
                        }
                        ctx.request_repaint();
                    }
                })
            };
            let h1 = hijo.stdout.take().map(|s| leer(Box::new(s)));
            let h2 = hijo.stderr.take().map(|s| leer(Box::new(s)));
            let fin = hijo.wait();
            for h in [h1, h2].into_iter().flatten() {
                let _ = h.join();
            }
            let e = match fin {
                Ok(s) if s.success() => Estado::Instalada,
                Ok(s) => Estado::Error(format!("el instalador terminó con error ({s})")),
                Err(e) => Estado::Error(e.to_string()),
            };
            Self::poner(&estado, e, &ctx);
        });
    }
}

/// La `grafo.app` desde la que se ejecuta (solo macOS).
fn app_mac() -> Option<PathBuf> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    let exe = std::env::current_exe().ok()?;
    exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")).map(PathBuf::from)
}

/// Programa a abrir al reiniciar: la app (macOS), el instalado en ~/.local/bin
/// si es el que corre, o el ejecutable actual.
fn destino_reinicio() -> Option<PathBuf> {
    if let Some(app) = app_mac() {
        return Some(app);
    }
    let exe = std::env::current_exe().ok()?;
    // En Linux, si el ejecutable se reemplazó, la ruta acaba en « (deleted)».
    let texto = exe.to_string_lossy();
    Some(PathBuf::from(texto.strip_suffix(" (deleted)").unwrap_or(&texto).to_string()))
}

/// Lanza una copia nueva cuando esta termine y cierra la ventana.
pub fn reiniciar(ctx: &egui::Context) -> Result<(), String> {
    let destino = destino_reinicio().ok_or("no se encontró el ejecutable")?;
    let abrir = if app_mac().is_some() { "open \"$1\"" } else { "\"$1\" >/dev/null 2>&1 &" };
    let script = format!("while kill -0 {} 2>/dev/null; do sleep 0.2; done; {abrir}", std::process::id());
    Command::new("sh")
        .arg("-c")
        .arg(script)
        .arg("sh")
        .arg(&destino)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    Ok(())
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn quita_colores() {
        assert_eq!(super::sin_colores("\x1b[1;34m==>\x1b[0m Listo"), "==> Listo");
    }
}
