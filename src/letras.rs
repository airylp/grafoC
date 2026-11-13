//! Tipos de letra de la interfaz: las que traiga el sistema (SF Pro, Helvetica,
//! Avenir…) más la incluida en egui. Detrás de la elegida van siempre las de
//! egui, para los símbolos y emojis que no tenga.

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId, TextStyle};

pub struct Opcion {
    pub nombre: &'static str,
    /// Rutas posibles (la primera que exista). Vacío = incluida en el programa.
    rutas: &'static [&'static str],
}

pub const INCLUIDA: &str = "Ubuntu Light (incluida)";

/// En macOS la del sistema; en otros, la incluida.
pub const PREDETERMINADA: &str = if cfg!(target_os = "macos") { "SF Pro" } else { INCLUIDA };

pub const OPCIONES: &[Opcion] = &[
    Opcion { nombre: INCLUIDA, rutas: &[] },
    Opcion { nombre: "SF Pro", rutas: &["/System/Library/Fonts/SFNS.ttf"] },
    Opcion { nombre: "Helvetica Neue", rutas: &["/System/Library/Fonts/HelveticaNeue.ttc"] },
    Opcion { nombre: "Helvetica", rutas: &["/System/Library/Fonts/Helvetica.ttc"] },
    Opcion { nombre: "Avenir", rutas: &["/System/Library/Fonts/Avenir.ttc"] },
    Opcion { nombre: "Avenir Next", rutas: &["/System/Library/Fonts/Avenir Next.ttc"] },
    Opcion { nombre: "Menlo", rutas: &["/System/Library/Fonts/Menlo.ttc"] },
    Opcion { nombre: "Segoe UI", rutas: &["C:\\Windows\\Fonts\\segoeui.ttf"] },
    Opcion {
        nombre: "Inter",
        rutas: &["/usr/share/fonts/opentype/inter/Inter-Regular.otf", "/usr/share/fonts/truetype/inter/Inter-Regular.ttf"],
    },
    Opcion { nombre: "Cantarell", rutas: &["/usr/share/fonts/opentype/cantarell/Cantarell-Regular.otf"] },
    Opcion { nombre: "DejaVu Sans", rutas: &["/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"] },
    Opcion { nombre: "Noto Sans", rutas: &["/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf"] },
    Opcion { nombre: "Liberation Sans", rutas: &["/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf"] },
];

impl Opcion {
    fn archivo(&self) -> Option<&'static str> {
        self.rutas.iter().copied().find(|p| std::path::Path::new(p).exists())
    }
}

/// Las que se pueden usar en este equipo.
pub fn disponibles() -> Vec<&'static str> {
    OPCIONES
        .iter()
        .filter(|o| o.rutas.is_empty() || o.archivo().is_some())
        .map(|o| o.nombre)
        .collect()
}

/// Fuentes con `nombre` delante de las de egui. Si no se puede cargar, quedan las de egui.
pub fn definiciones(nombre: &str) -> FontDefinitions {
    let mut defs = FontDefinitions::default();
    let datos = OPCIONES
        .iter()
        .find(|o| o.nombre == nombre)
        .and_then(|o| o.archivo())
        .and_then(|p| std::fs::read(p).ok());
    if let Some(bytes) = datos {
        defs.font_data.insert("elegida".into(), Arc::new(FontData::from_owned(bytes)));
        defs.families.entry(FontFamily::Proportional).or_default().insert(0, "elegida".into());
    }
    defs
}

/// Tamaños discretos a partir del tamaño base del texto.
pub fn aplicar_tamanos(ctx: &egui::Context, base: f32) {
    for tema in [egui::Theme::Dark, egui::Theme::Light] {
        ctx.style_mut_of(tema, |s| {
            s.text_styles = [
                (TextStyle::Small, FontId::proportional(base * 0.82)),
                (TextStyle::Body, FontId::proportional(base)),
                (TextStyle::Button, FontId::proportional(base)),
                (TextStyle::Heading, FontId::proportional(base * 1.3)),
                (TextStyle::Monospace, FontId::monospace(base * 0.95)),
            ]
            .into();
        });
    }
}
