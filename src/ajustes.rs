//! Ajustes persistentes en un archivo de texto `clave=valor`
//! (`~/Library/Application Support/grafo/ajustes.txt` en macOS,
//! `~/.config/grafo/ajustes.txt` en Linux).

use std::path::PathBuf;

use eframe::egui::ThemePreference;

#[derive(Clone, PartialEq)]
pub struct Ajustes {
    pub tema: ThemePreference,
    /// Nombre de `letras::OPCIONES`.
    pub letra: String,
    pub tam_letra: f32,
    /// Forma con que abre: árbol (si no, fuerzas).
    pub arbol: bool,
    pub etiquetas: bool,
    pub giro_auto: bool,
    /// Velocidad del giro automático (radianes por segundo).
    pub vel_giro: f32,
    pub rejilla: bool,
    /// Multiplicador del tamaño de las figuras.
    pub tam_figuras: f32,
    /// Atenuar lo lejano.
    pub niebla: bool,
    /// Color de fondo de la vista 3D; `None` = el del tema.
    pub fondo: Option<[u8; 3]>,
    /// Tipos visibles al abrir (orden de `Tipo::TODOS`).
    pub mostrar: [bool; 6],
    /// Antigüedad máxima en días (0 = todo).
    pub dias_max: u32,
    /// Carpeta de Claude; vacía = `CLAUDE_CONFIG_DIR` o `~/.claude`.
    pub carpeta: String,
    /// Idioma elegido en el instalador (se conserva para él).
    pub idioma: Option<String>,
}

impl Default for Ajustes {
    fn default() -> Self {
        Ajustes {
            tema: ThemePreference::Dark,
            letra: crate::letras::PREDETERMINADA.into(),
            tam_letra: 12.5,
            arbol: true,
            etiquetas: true,
            giro_auto: false,
            vel_giro: 0.15,
            rejilla: true,
            tam_figuras: 1.0,
            niebla: true,
            fondo: None,
            mostrar: [true; 6],
            dias_max: 0,
            carpeta: String::new(),
            idioma: None,
        }
    }
}

/// Carpeta de configuración de la aplicación.
pub fn carpeta_config() -> Option<PathBuf> {
    let env = |k| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support"))
    } else if cfg!(windows) {
        env("APPDATA")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))
    }?;
    Some(base.join("grafo"))
}

fn ruta() -> Option<PathBuf> {
    Some(carpeta_config()?.join("ajustes.txt"))
}

fn si_no(b: bool) -> &'static str {
    if b { "si" } else { "no" }
}

impl Ajustes {
    pub fn a_texto(&self) -> String {
        let tema = match self.tema {
            ThemePreference::Light => "claro",
            ThemePreference::Dark => "oscuro",
            ThemePreference::System => "sistema",
        };
        let mostrar: String = self.mostrar.iter().map(|&b| if b { '1' } else { '0' }).collect();
        let mut t = format!(
            "tema={tema}\nletra={}\ntam_letra={}\nforma={}\netiquetas={}\ngiro={}\nvel_giro={}\nrejilla={}\ntam_figuras={}\nniebla={}\ntipos={mostrar}\ndias_max={}\ncarpeta={}\n",
            self.letra,
            self.tam_letra,
            if self.arbol { "arbol" } else { "fuerzas" },
            si_no(self.etiquetas),
            si_no(self.giro_auto),
            self.vel_giro,
            si_no(self.rejilla),
            self.tam_figuras,
            si_no(self.niebla),
            self.dias_max,
            self.carpeta.replace('\n', " "),
        );
        if let Some([r, g, b]) = self.fondo {
            t += &format!("fondo=#{r:02x}{g:02x}{b:02x}\n");
        }
        if let Some(i) = &self.idioma {
            t += &format!("idioma={i}\n");
        }
        t
    }

    pub fn de_texto(texto: &str) -> Self {
        let mut a = Ajustes::default();
        for linea in texto.lines() {
            let Some((k, v)) = linea.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "tema" => {
                    a.tema = match v {
                        "claro" => ThemePreference::Light,
                        "sistema" => ThemePreference::System,
                        _ => ThemePreference::Dark,
                    }
                }
                "letra" if !v.is_empty() => a.letra = v.into(),
                "tam_letra" => a.tam_letra = v.parse::<f32>().unwrap_or(12.5).clamp(9.0, 20.0),
                "forma" => a.arbol = v != "fuerzas",
                "etiquetas" => a.etiquetas = v != "no",
                "giro" => a.giro_auto = v == "si",
                "vel_giro" => a.vel_giro = v.parse::<f32>().unwrap_or(0.15).clamp(0.02, 1.0),
                "rejilla" => a.rejilla = v != "no",
                "tam_figuras" => a.tam_figuras = v.parse::<f32>().unwrap_or(1.0).clamp(0.4, 2.5),
                "niebla" => a.niebla = v != "no",
                "tipos" => {
                    for (i, c) in v.chars().take(6).enumerate() {
                        a.mostrar[i] = c != '0';
                    }
                }
                "fondo" => a.fondo = color_hex(v),
                "dias_max" => a.dias_max = v.parse::<u32>().unwrap_or(0).min(365),
                "carpeta" => a.carpeta = v.into(),
                "idioma" if !v.is_empty() => a.idioma = Some(v.into()),
                _ => {}
            }
        }
        a
    }

    pub fn cargar() -> Self {
        ruta()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::de_texto(&t))
            .unwrap_or_default()
    }

    pub fn guardar(&self) -> Result<(), String> {
        let p = ruta().ok_or("no se encontró la carpeta de configuración")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        // Temporal + renombrar: nunca queda a medias.
        let tmp = p.with_extension("txt.tmp");
        std::fs::write(&tmp, self.a_texto())
            .and_then(|_| std::fs::rename(&tmp, &p))
            .map_err(|e| e.to_string())
    }

    /// Carpeta de Claude que se va a leer.
    pub fn carpeta_claude(&self) -> PathBuf {
        let c = self.carpeta.trim();
        if c.is_empty() {
            return crate::datos::dir_claude();
        }
        match c.strip_prefix("~/") {
            Some(resto) => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(resto),
            None => PathBuf::from(c),
        }
    }
}

/// `#rrggbb` → [r, g, b].
fn color_hex(v: &str) -> Option<[u8; 3]> {
    let h = v.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn ida_y_vuelta() {
        let a = Ajustes {
            tema: ThemePreference::Light,
            letra: "Helvetica".into(),
            tam_letra: 14.0,
            arbol: false,
            etiquetas: false,
            giro_auto: true,
            vel_giro: 0.3,
            rejilla: false,
            tam_figuras: 1.5,
            niebla: false,
            fondo: Some([10, 200, 255]),
            mostrar: [true, false, true, true, false, true],
            dias_max: 30,
            carpeta: "~/otra/.claude".into(),
            idioma: Some("en".into()),
        };
        assert!(Ajustes::de_texto(&a.a_texto()) == a);
        assert!(Ajustes::de_texto("") == Ajustes::default());
    }
}
