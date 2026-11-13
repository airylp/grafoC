//! Interfaz: panel lateral con filtros y listas, vista 3D al centro y detalle a la derecha.

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::thread;

use chrono::Local;
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, PointerButton, Pos2, Rect, RichText, Sense, Stroke,
    Vec2,
};

use crate::ajustes::Ajustes;
use crate::datos::{self, Datos, Mensaje, Tipo, hace};
use crate::letras;
use crate::espacio::{Camara, Disposicion, V3, disposicion_arbol};
use crate::figuras;

pub struct App {
    rx: Option<Receiver<Mensaje>>,
    progreso: String,
    datos: Datos,
    padre: Vec<Option<usize>>,
    vecinos: Vec<Vec<usize>>,
    disp: Option<Disposicion>,
    camara: Camara,
    /// Ajustes (también los filtros y opciones del panel, que se recuerdan).
    ajustes: Ajustes,
    /// Lo último guardado en disco (se guarda al cambiar).
    guardados: Ajustes,
    /// Letra y tamaño aplicados a egui (para no reconstruir las fuentes cada fotograma).
    letra_aplicada: (String, f32),
    letras_disponibles: Vec<&'static str>,
    ver_ajustes: bool,
    error_ajustes: Option<String>,
    /// Carpeta de Claude leída en la última carga.
    carpeta_leida: std::path::PathBuf,
    busqueda: String,
    seleccion: Option<usize>,
    /// Ver solo la rama de este nodo (más el camino hasta la raíz).
    rama: Option<usize>,
    /// Nodo sobre el que se abrió el menú contextual.
    menu_nodo: Option<usize>,
    arrastrando_nodo: Option<usize>,
    /// Encuadra el grafo automáticamente hasta que la persona mueva la cámara.
    encuadre_auto: bool,
    /// Visibilidad y modo con que se calculó el árbol (para recalcularlo al cambiar).
    arbol_calculado: Option<(Vec<bool>, bool)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let ajustes = Ajustes::cargar();
        preparar_temas(&cc.egui_ctx);
        let mut app = App {
            rx: None,
            progreso: String::new(),
            datos: Datos::default(),
            padre: Vec::new(),
            vecinos: Vec::new(),
            disp: None,
            camara: if ajustes.arbol { Camara::de_frente() } else { Camara::default() },
            guardados: ajustes.clone(),
            ajustes,
            letra_aplicada: (String::new(), 0.0),
            letras_disponibles: letras::disponibles(),
            // Para capturas de prueba: abrir con la ventana de ajustes visible.
            ver_ajustes: std::env::var_os("GRAFO_VER_AJUSTES").is_some(),
            error_ajustes: None,
            carpeta_leida: Default::default(),
            busqueda: String::new(),
            seleccion: None,
            rama: None,
            menu_nodo: None,
            arrastrando_nodo: None,
            encuadre_auto: true,
            arbol_calculado: None,
        };
        app.recargar();
        app
    }

    fn recargar(&mut self) {
        let (tx, rx) = channel();
        let carpeta = self.ajustes.carpeta_claude();
        self.carpeta_leida = carpeta.clone();
        thread::spawn(move || datos::cargar(carpeta, tx));
        self.rx = Some(rx);
        self.progreso = "Iniciando…".into();
    }

    fn recibir(&mut self) {
        let Some(rx) = &self.rx else { return };
        while let Ok(m) = rx.try_recv() {
            match m {
                Mensaje::Progreso(p) => self.progreso = p,
                Mensaje::Listo(d) => {
                    let n = d.nodos.len();
                    let mut padre = vec![None; n];
                    let mut vecinos = vec![Vec::new(); n];
                    for &(a, b) in &d.aristas {
                        if padre[b].is_none() {
                            padre[b] = Some(a);
                        }
                        vecinos[a].push(b);
                        vecinos[b].push(a);
                    }
                    self.disp = Some(Disposicion::nueva(&d));
                    self.padre = padre;
                    self.vecinos = vecinos;
                    self.datos = d;
                    self.seleccion = None;
                    self.rama = None;
                    self.menu_nodo = None;
                    self.encuadre_auto = true;
                    self.arbol_calculado = None;
                    self.rx = None;
                    self.progreso.clear();
                    // Para capturas de prueba: abrir viendo solo la rama con esa etiqueta.
                    if let Ok(e) = std::env::var("GRAFO_RAMA") {
                        self.rama = self.datos.nodos.iter().position(|n| n.etiqueta == e);
                    }
                    return;
                }
            }
        }
    }

    fn mostrar_tipo(&self, t: Tipo) -> bool {
        self.ajustes.mostrar[Tipo::TODOS.iter().position(|&x| x == t).unwrap_or(0)]
    }

    /// Un nodo es visible si su tipo está activo, cumple la antigüedad y su padre es visible.
    fn visibles(&self) -> Vec<bool> {
        let n = self.datos.nodos.len();
        let mut vis = vec![false; n];
        let ahora = Local::now();
        // Los padres siempre tienen un id menor que sus hijos.
        for i in 0..n {
            let nodo = &self.datos.nodos[i];
            let mut v = self.mostrar_tipo(nodo.tipo);
            if v && self.ajustes.dias_max > 0
                && matches!(nodo.tipo, Tipo::Sesion | Tipo::Archivo | Tipo::Pendiente)
            {
                v = nodo
                    .fecha
                    .is_some_and(|f| (ahora - f).num_days() < self.ajustes.dias_max as i64);
            }
            if let Some(p) = self.padre[i] {
                v = v && vis[p];
            }
            vis[i] = v;
        }
        if let Some(r) = self.rama.filter(|&r| r < n) {
            // La rama: el nodo y sus descendientes (los padres tienen id menor),
            // más el camino hasta la raíz para que el árbol conserve su tronco.
            let mut en_rama = vec![false; n];
            en_rama[r] = true;
            for i in r + 1..n {
                en_rama[i] = self.padre[i].is_some_and(|p| en_rama[p]);
            }
            let mut a = self.padre[r];
            while let Some(p) = a {
                en_rama[p] = true;
                a = self.padre[p];
            }
            for i in 0..n {
                vis[i] &= en_rama[i];
            }
        }
        vis
    }

    /// Entra en la rama de `i` (o vuelve a todo con `None`) y reencuadra.
    fn ver_rama(&mut self, i: Option<usize>) {
        self.rama = i;
        self.encuadre_auto = true;
        if !self.ajustes.arbol {
            self.camara.destino = V3::CERO;
            if let Some(d) = &mut self.disp {
                d.reactivar();
            }
        }
    }

    /// Paleta de la vista 3D: la del tema o, con fondo propio, la que contraste con él.
    fn paleta_vista(&self, tema_oscuro: bool) -> (Paleta, bool) {
        match self.ajustes.fondo {
            Some([r, g, b]) => {
                let oscuro = 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * (b as f32) < 128.0;
                let mut p = Paleta::de(oscuro);
                p.fondo = Color32::from_rgb(r, g, b);
                (p, oscuro)
            }
            None => (Paleta::de(tema_oscuro), tema_oscuro),
        }
    }

    fn centrar_camara(&mut self) {
        self.camara = if self.ajustes.arbol { Camara::de_frente() } else { Camara::default() };
        self.encuadre_auto = true;
    }

    /// Recalcula el árbol si cambió lo visible o el modo; en modo fuerzas reanuda la simulación.
    fn actualizar_disposicion(&mut self, visibles: &[bool]) {
        let Some(disp) = &mut self.disp else { return };
        let actual = Some((visibles.to_vec(), self.ajustes.arbol));
        if self.arbol_calculado == actual {
            return;
        }
        if self.ajustes.arbol {
            disp.meta = Some(disposicion_arbol(&self.datos, visibles, &self.padre));
        } else {
            disp.meta = None;
            disp.reactivar();
        }
        self.arbol_calculado = actual;
    }

    fn coincide(&self, i: usize, q: &str) -> bool {
        let n = &self.datos.nodos[i];
        n.etiqueta.to_lowercase().contains(q)
            || n.detalle.iter().any(|(_, v)| v.to_lowercase().contains(q))
    }

    fn enfocar(&mut self, i: usize) {
        self.seleccion = Some(i);
        if let Some(d) = &self.disp {
            self.camara.destino = d.pos[i];
        }
    }
}

fn color(t: Tipo) -> Color32 {
    match t {
        Tipo::Raiz => Color32::from_rgb(255, 214, 120),
        Tipo::Carpeta => Color32::from_rgb(105, 155, 255),
        Tipo::Proyecto => Color32::from_rgb(80, 220, 160),
        Tipo::Sesion => Color32::from_rgb(185, 130, 255),
        Tipo::Archivo => Color32::from_rgb(255, 165, 75),
        Tipo::Pendiente => Color32::from_rgb(255, 85, 110),
    }
}

fn radio_base(t: Tipo) -> f32 {
    match t {
        Tipo::Raiz => 16.0,
        Tipo::Carpeta => 9.0,
        Tipo::Proyecto => 11.0,
        Tipo::Sesion => 7.0,
        Tipo::Archivo => 6.5,
        Tipo::Pendiente => 7.0,
    }
}

fn mezclar(c: Color32, k: f32, fondo: Color32) -> Color32 {
    let m = |a: u8, b: u8| (a as f32 * k + b as f32 * (1.0 - k)).round() as u8;
    Color32::from_rgb(m(c.r(), fondo.r()), m(c.g(), fondo.g()), m(c.b(), fondo.b()))
}

/// ¿Es el polígono (en pantalla) tan delgado que su contorno se vería mal?
fn es_astilla(pts: &[Pos2]) -> bool {
    let n = pts.len();
    let mut area = 0.0;
    let mut perimetro = 0.0;
    for i in 0..n {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        area += a.x * b.y - b.x * a.y;
        perimetro += a.distance(b);
    }
    let area = area.abs() * 0.5;
    // Un cuadrado da 1/16; una astilla, casi 0.
    perimetro < 1.0 || area / (perimetro * perimetro) < 0.012
}

/// Multiplica el color (k > 1 aclara, k < 1 oscurece).
fn escalar(c: Color32, k: f32) -> Color32 {
    let m = |a: u8| (a as f32 * k).clamp(0.0, 255.0).round() as u8;
    Color32::from_rgb(m(c.r()), m(c.g()), m(c.b()))
}

fn abrir(p: &Path) {
    let programa = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
    let _ = Command::new(programa).arg(p).spawn();
}

/// Colores de la vista 3D según el tema.
struct Paleta {
    fondo: Color32,
    etiqueta: Color32,
    rejilla: Color32,
    globo_fondo: Color32,
    globo_texto: Color32,
    ayuda: Color32,
    seleccion: Color32,
    coincidencia: Color32,
    activa: Color32,
}

impl Paleta {
    fn de(oscuro: bool) -> Self {
        if oscuro {
            Paleta {
                fondo: Color32::from_rgb(13, 15, 21),
                etiqueta: Color32::from_rgb(196, 200, 212),
                rejilla: Color32::from_rgba_unmultiplied(90, 100, 130, 22),
                globo_fondo: Color32::from_rgba_unmultiplied(24, 27, 37, 240),
                globo_texto: Color32::from_rgb(208, 212, 222),
                ayuda: Color32::from_gray(92),
                seleccion: Color32::from_rgb(235, 238, 245),
                coincidencia: Color32::from_rgb(240, 205, 90),
                activa: Color32::from_rgb(110, 230, 140),
            }
        } else {
            Paleta {
                fondo: Color32::from_rgb(246, 247, 250),
                etiqueta: Color32::from_rgb(52, 56, 68),
                rejilla: Color32::from_rgba_unmultiplied(70, 80, 110, 30),
                globo_fondo: Color32::from_rgba_unmultiplied(255, 255, 255, 245),
                globo_texto: Color32::from_rgb(40, 44, 54),
                ayuda: Color32::from_gray(140),
                seleccion: Color32::from_rgb(30, 34, 44),
                coincidencia: Color32::from_rgb(200, 140, 0),
                activa: Color32::from_rgb(30, 160, 70),
            }
        }
    }
}

/// Color del tipo ajustado al tema (en claro, algo más oscuro para que contraste).
fn color_tema(t: Tipo, oscuro: bool) -> Color32 {
    if oscuro { color(t) } else { escalar(color(t), 0.82) }
}

/// Estilo propio de los dos temas: oscuro azulado y sobrio, claro limpio.
fn preparar_temas(ctx: &egui::Context) {
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        let v = &mut s.visuals;
        v.panel_fill = Color32::from_rgb(19, 21, 28);
        v.window_fill = Color32::from_rgb(23, 26, 34);
        v.extreme_bg_color = Color32::from_rgb(12, 14, 19);
        v.faint_bg_color = Color32::from_rgb(25, 28, 37);
        v.code_bg_color = Color32::from_rgb(28, 31, 40);
        v.window_stroke = Stroke::new(1.0, Color32::from_rgb(40, 44, 56));
        v.selection.bg_fill = Color32::from_rgb(48, 66, 110);
        v.selection.stroke = Stroke::new(1.0, Color32::from_rgb(150, 175, 235));
        v.hyperlink_color = Color32::from_rgb(130, 165, 240);
        v.override_text_color = Some(Color32::from_rgb(176, 181, 194));
        let w = &mut v.widgets;
        w.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(36, 40, 51));
        w.noninteractive.bg_fill = Color32::from_rgb(19, 21, 28);
        for (estado, fondo) in [
            (&mut w.inactive, Color32::from_rgb(32, 36, 46)),
            (&mut w.hovered, Color32::from_rgb(42, 47, 60)),
            (&mut w.active, Color32::from_rgb(52, 58, 74)),
            (&mut w.open, Color32::from_rgb(36, 40, 51)),
        ] {
            estado.bg_fill = fondo;
            estado.weak_bg_fill = fondo;
            estado.bg_stroke = Stroke::new(if fondo.r() > 40 { 1.0 } else { 0.0 }, Color32::from_rgb(70, 78, 98));
            estado.corner_radius = CornerRadius::same(5);
        }
        s.spacing.item_spacing = Vec2::new(7.0, 5.0);
    });
    ctx.style_mut_of(egui::Theme::Light, |s| {
        let v = &mut s.visuals;
        v.panel_fill = Color32::from_rgb(250, 250, 252);
        v.window_fill = Color32::from_rgb(252, 252, 253);
        v.override_text_color = Some(Color32::from_rgb(60, 64, 74));
        for estado in [&mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
            estado.corner_radius = CornerRadius::same(5);
        }
        s.spacing.item_spacing = Vec2::new(7.0, 5.0);
    });
}

/// Color de las ramas gruesas del árbol.
const TRONCO: Color32 = Color32::from_rgb(150, 112, 78);

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.aplicar_ajustes(ui.ctx());
        self.recibir();
        if self.rx.is_some() {
            ui.ctx().request_repaint();
        }
        let visibles = self.visibles();
        self.actualizar_disposicion(&visibles);
        let q = self.busqueda.trim().to_lowercase();
        let mut enfocar: Option<usize> = None;

        // ------------------------------------------------------------ izquierda
        egui::Panel::left("panel_izq")
            .resizable(true)
            .default_size(320.0)
            .show(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.heading("Grafo de Claude");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.selectable_label(self.ver_ajustes, "⚙").on_hover_text("Ajustes").clicked() {
                            self.ver_ajustes = !self.ver_ajustes;
                        }
                    });
                });
                ui.label(RichText::new(datos::ruta_corta(&self.carpeta_leida)).small().weak());
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    if ui.button("⟳ Recargar").clicked() && self.rx.is_none() {
                        self.recargar();
                    }
                    if ui.button("⌖ Centrar").clicked() {
                        self.centrar_camara();
                    }
                    if ui.button("✦ Reordenar").clicked() {
                        if let Some(d) = &mut self.disp {
                            d.reactivar();
                        }
                    }
                });
                if let Some(r) = self.rama.filter(|&r| r < self.datos.nodos.len()) {
                    let mut volver = false;
                    egui::Frame::new()
                        .fill(ui.visuals().selection.bg_fill.gamma_multiply(0.35))
                        .corner_radius(CornerRadius::same(5))
                        .inner_margin(6.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Rama: {}", datos::recortar(&self.datos.nodos[r].etiqueta, 28))).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    volver = ui.button("↩ Ver todo").on_hover_text("Esc").clicked();
                                });
                            });
                        });
                    if volver {
                        self.ver_rama(None);
                    }
                }
                if !self.progreso.is_empty() {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new(&self.progreso).small());
                    });
                }
                ui.separator();

                let mut cambio = false;
                for (k, t) in Tipo::TODOS.iter().enumerate() {
                    let cuenta = self.datos.nodos.iter().filter(|n| n.tipo == *t).count();
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 4.5, color_tema(*t, ui.visuals().dark_mode));
                        let ant = self.ajustes.mostrar[k];
                        ui.checkbox(&mut self.ajustes.mostrar[k], format!("{} ({cuenta})", t.nombre()));
                        cambio |= ant != self.ajustes.mostrar[k];
                    });
                }
                ui.add_space(4.0);
                let ant = self.ajustes.dias_max;
                ui.add(
                    egui::Slider::new(&mut self.ajustes.dias_max, 0..=90)
                        .text("días máx.")
                        .custom_formatter(|v, _| {
                            if v == 0.0 { "todo".into() } else { format!("{v}") }
                        }),
                );
                cambio |= ant != self.ajustes.dias_max;
                if cambio {
                    if let Some(d) = &mut self.disp {
                        d.reactivar();
                    }
                }
                ui.horizontal(|ui| {
                    ui.label("Forma:");
                    let ant = self.ajustes.arbol;
                    ui.selectable_value(&mut self.ajustes.arbol, true, "🌳 Árbol");
                    ui.selectable_value(&mut self.ajustes.arbol, false, "✦ Fuerzas");
                    if ant != self.ajustes.arbol {
                        self.centrar_camara();
                    }
                });
                ui.checkbox(&mut self.ajustes.etiquetas, "Mostrar etiquetas");
                ui.checkbox(&mut self.ajustes.giro_auto, "Giro automático");
                ui.add_space(4.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.busqueda)
                        .hint_text("Buscar… (Enter enfoca)")
                        .desired_width(f32::INFINITY),
                );
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !q.is_empty()
                {
                    enfocar = (0..self.datos.nodos.len())
                        .find(|&i| visibles[i] && self.coincide(i, &q));
                }
                ui.separator();

                let alto = (ui.available_height() - 60.0).max(120.0) / 2.0;
                ui.label(RichText::new("Últimos archivos modificados").strong());
                egui::ScrollArea::vertical()
                    .id_salt("recientes")
                    .max_height(alto)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let en_rama = |id: usize| self.rama.is_none() || visibles[id];
                        for &id in self.datos.recientes.iter().filter(|&&id| en_rama(id)).take(80) {
                            let n = &self.datos.nodos[id];
                            let proy = n
                                .proyecto
                                .map(|p| self.datos.nodos[p].etiqueta.as_str())
                                .unwrap_or("");
                            let txt = format!(
                                "{}  ·  {}  ·  {}",
                                n.etiqueta,
                                proy,
                                n.fecha.map(hace).unwrap_or_default()
                            );
                            let r = ui.selectable_label(self.seleccion == Some(id), txt);
                            let r = match &n.ruta {
                                Some(p) => r.on_hover_text(p.display().to_string()),
                                None => r,
                            };
                            if r.clicked() {
                                enfocar = Some(id);
                            }
                        }
                    });
                ui.separator();
                ui.label(
                    RichText::new(format!(
                        "Pendientes ({})",
                        self.datos.pendientes.iter().filter(|&&id| self.rama.is_none() || visibles[id]).count()
                    ))
                    .strong(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("pendientes")
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        if self.datos.pendientes.is_empty() && self.rx.is_none() {
                            ui.label(
                                RichText::new("Sin TODO/FIXME ni cambios sin commit.").weak(),
                            );
                        }
                        for &id in self.datos.pendientes.iter().filter(|&&id| self.rama.is_none() || visibles[id]) {
                            let n = &self.datos.nodos[id];
                            let proy = n
                                .proyecto
                                .map(|p| self.datos.nodos[p].etiqueta.as_str())
                                .unwrap_or("");
                            let r = ui.selectable_label(
                                self.seleccion == Some(id),
                                format!("[{proy}] {}", n.etiqueta),
                            );
                            if r.clicked() {
                                enfocar = Some(id);
                            }
                        }
                    });
            });

        // ------------------------------------------------------------ derecha
        let mut abrir_ruta = None;
        if let Some(sel) = self.seleccion.filter(|&s| s < self.datos.nodos.len()) {
            let mut cerrar = false;
            egui::Panel::right("panel_der")
                .resizable(true)
                .default_size(300.0)
                .show(ui, |ui| {
                    let n = &self.datos.nodos[sel];
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(14.0), Sense::hover());
                        ui.painter().circle_filled(r.center(), 5.0, color_tema(n.tipo, ui.visuals().dark_mode));
                        ui.label(RichText::new(n.tipo.nombre()).small().weak());
                        if ui.small_button("✕").clicked() {
                            cerrar = true;
                        }
                    });
                    ui.label(RichText::new(&n.etiqueta).heading());
                    if n.activa {
                        ui.colored_label(Paleta::de(ui.visuals().dark_mode).activa, "● Sesión en curso");
                    }
                    if let Some(f) = n.fecha {
                        ui.label(format!("{}  ({})", f.format("%Y-%m-%d %H:%M"), hace(f)));
                    }
                    ui.separator();
                    egui::ScrollArea::vertical().id_salt("detalle").show(ui, |ui| {
                        egui::Grid::new("grid_detalle")
                            .num_columns(2)
                            .spacing([8.0, 6.0])
                            .striped(true)
                            .show(ui, |ui| {
                                for (k, v) in &n.detalle {
                                    ui.label(RichText::new(k).weak());
                                    ui.add(egui::Label::new(v).wrap());
                                    ui.end_row();
                                }
                            });
                        if let Some(p) = &n.ruta {
                            ui.add_space(6.0);
                            if ui
                                .button("📂 Abrir")
                                .on_hover_text(p.display().to_string())
                                .clicked()
                            {
                                abrir_ruta = Some(p.clone());
                            }
                        }
                        ui.separator();
                        ui.label(RichText::new("Conectado con").strong());
                        let mut vs: Vec<usize> = self.vecinos[sel].clone();
                        vs.sort_by_key(|&v| {
                            let t = self.datos.nodos[v].tipo;
                            (Tipo::TODOS.iter().position(|&x| x == t), v)
                        });
                        vs.dedup();
                        for v in vs {
                            let m = &self.datos.nodos[v];
                            let txt =
                                RichText::new(format!("● {}", m.etiqueta)).color(color_tema(m.tipo, ui.visuals().dark_mode));
                            if ui.selectable_label(false, txt).clicked() {
                                enfocar = Some(v);
                            }
                        }
                    });
                });
            if cerrar {
                self.seleccion = None;
            }
        }
        if let Some(p) = abrir_ruta {
            abrir(&p);
        }

        self.ventana_ajustes(ui.ctx());

        // ------------------------------------------------------------ centro
        let fondo = self.paleta_vista(ui.visuals().dark_mode).0.fondo;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(fondo))
            .show(ui, |ui| {
                self.vista(ui, &visibles, &q, &mut enfocar);
            });

        if let Some(i) = enfocar {
            self.encuadre_auto = false;
            self.enfocar(i);
        }
        self.guardar_si_cambio(ui.ctx());
    }
}

impl App {
    fn vista(&mut self, ui: &mut egui::Ui, visibles: &[bool], q: &str, enfocar: &mut Option<usize>) {
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        let dt = ui.input(|i| i.stable_dt).min(0.1);
        let (pal, oscuro) = self.paleta_vista(ui.visuals().dark_mode);
        let fondo = pal.fondo;
        let color = |t: Tipo| color_tema(t, oscuro);

        if self.datos.nodos.is_empty() {
            let txt = if self.progreso.is_empty() { "Sin datos" } else { self.progreso.as_str() };
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                txt,
                FontId::proportional(15.0),
                pal.ayuda,
            );
            return;
        }

        // --- Simulación ---
        let mut moviendo = false;
        if let Some(d) = &mut self.disp {
            for _ in 0..2 {
                moviendo |= d.paso(&self.datos, visibles, self.arrastrando_nodo);
            }
        }

        // --- Entrada de cámara (zoom) ---
        if self.ajustes.giro_auto && !resp.dragged() {
            self.camara.guinada += dt * self.ajustes.vel_giro;
            moviendo = true;
        }
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if scroll != 0.0 || zoom != 1.0 {
                self.encuadre_auto = false;
            }
            if scroll != 0.0 {
                self.camara.acercar((-scroll * 0.0025).exp());
            }
            if zoom != 1.0 {
                self.camara.acercar(1.0 / zoom);
            }
        }
        moviendo |= self.camara.animar(dt);

        // Encuadre automático: centra la cámara en la caja que ocupan los nodos visibles
        // y la aleja hasta que quepa entera.
        let caja = self.disp.as_ref().map(|d| {
            let mut min = V3::new(f32::MAX, f32::MAX, f32::MAX);
            let mut max = V3::new(f32::MIN, f32::MIN, f32::MIN);
            // Con árbol se usan las posiciones finales, para no perseguir la animación.
            let pos = d.meta.as_ref().unwrap_or(&d.pos);
            for i in (0..pos.len()).filter(|&i| visibles[i]) {
                let p = pos[i];
                min = V3::new(min.x.min(p.x), min.y.min(p.y), min.z.min(p.z));
                max = V3::new(max.x.max(p.x), max.y.max(p.y), max.z.max(p.z));
            }
            (min, max)
        });
        if let (true, Some((min, max))) = (self.encuadre_auto, caja) {
            let centro = (min + max) * 0.5;
            let medio = (max - min) * 0.5;
            let tan = (self.camara.fov * 0.5).tan();
            let aspecto = rect.width() / rect.height();
            // Medio ancho visto desde el frente/lado según hacia dónde mire la cámara.
            let (s, c) = self.camara.guinada.sin_cos();
            let ancho = (medio.x * c).abs() + (medio.z * s).abs();
            let hondo = (medio.x * s).abs() + (medio.z * c).abs();
            let alto = medio.y * self.camara.cabeceo.cos() + hondo * self.camara.cabeceo.sin().abs();
            let meta = (ancho / (tan * aspecto)).max(alto / tan).max(150.0) * 1.08 + hondo;
            let k = 1.0 - (-dt * 3.0).exp();
            self.camara.distancia += (meta - self.camara.distancia) * k;
            self.camara.destino = centro;
        }

        let Some(disp) = &self.disp else { return };
        let proy = self
            .camara
            .proyeccion(rect.center().x, rect.center().y, rect.height());

        // --- Proyección ---
        struct Dib {
            i: usize,
            p: Pos2,
            z: f32,
            r: f32,
        }
        let mut dibs: Vec<Dib> = Vec::new();
        let mut pant: Vec<Option<(Pos2, f32)>> = vec![None; self.datos.nodos.len()];
        for (i, n) in self.datos.nodos.iter().enumerate() {
            if !visibles[i] {
                continue;
            }
            if let Some((x, y, z, k)) = proy.proyectar(disp.pos[i]) {
                let r = (radio_base(n.tipo) * n.peso * k * 1.6 * self.ajustes.tam_figuras).clamp(1.5, 90.0);
                let p = Pos2::new(x, y);
                pant[i] = Some((p, z));
                dibs.push(Dib { i, p, z, r });
            }
        }

        // --- Nodo bajo el ratón ---
        let raton = resp.hover_pos().or(resp.interact_pointer_pos());
        let hover = raton.and_then(|m| {
            dibs.iter()
                .filter(|d| d.p.distance(m) <= d.r.max(6.0))
                .min_by(|a, b| a.z.total_cmp(&b.z))
                .map(|d| d.i)
        });

        // Arrastre: izquierdo sobre un nodo lo mueve; en vacío gira; derecho/medio desplaza.
        if resp.drag_started_by(PointerButton::Primary) {
            self.arrastrando_nodo = hover.filter(|&h| h != 0);
        }
        if resp.drag_stopped() {
            self.arrastrando_nodo = None;
        }
        let delta = resp.drag_delta();
        if resp.dragged() {
            self.encuadre_auto = false;
        }
        if resp.dragged_by(PointerButton::Primary) {
            if let (Some(i), Some(d)) = (self.arrastrando_nodo, self.disp.as_mut()) {
                let k = (d.pos[i] - proy.ojo).largo() / proy.focal;
                d.pos[i] += proy.derecha * (delta.x * k) - proy.arriba * (delta.y * k);
                d.reactivar();
            } else {
                self.camara.rotar(delta.x, delta.y);
            }
        } else if resp.dragged_by(PointerButton::Secondary) || resp.dragged_by(PointerButton::Middle)
        {
            self.camara.desplazar(delta.x, delta.y, rect.height());
        }
        if resp.clicked() {
            self.seleccion = hover;
        }
        if resp.secondary_clicked() {
            self.menu_nodo = hover;
        }
        self.menu_contextual(&resp);
        if self.rama.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) && !ui.ctx().egui_wants_keyboard_input() {
            self.ver_rama(None);
        }
        if resp.double_clicked() {
            if let Some(h) = hover {
                *enfocar = Some(h);
            }
        }

        // --- Resaltado ---
        let foco: Option<HashSet<usize>> = if let Some(s) = self.seleccion.or(hover) {
            let mut set: HashSet<usize> = self.vecinos[s].iter().copied().collect();
            set.insert(s);
            Some(set)
        } else if !q.is_empty() {
            Some(
                (0..self.datos.nodos.len())
                    .filter(|&i| visibles[i] && self.coincide(i, q))
                    .collect(),
            )
        } else {
            None
        };
        let resaltado = |i: usize| foco.as_ref().is_none_or(|f| f.contains(&i));

        let dist = self.camara.distancia;
        let con_niebla = self.ajustes.niebla;
        let niebla = |z: f32| if con_niebla { (1.25 - (z - dist) / (dist * 2.2)).clamp(0.25, 1.0) } else { 1.0 };

        // --- Rejilla del suelo ---
        let Some(disp) = &self.disp else { return };
        let (min, max) = caja.unwrap_or((V3::CERO, V3::CERO));
        let centro = (min + max) * 0.5;
        let lado = ((max.x - min.x).max(max.z - min.z) * 0.6).max(300.0);
        let paso = lado / 8.0;
        let piso = min.y - lado * 0.06;
        let cerca = self.camara.distancia * 0.05;
        let filas = if self.ajustes.rejilla { 8 } else { -1 };
        for k in -filas..=filas {
            let a = k as f32 * paso;
            for (p0, p1) in [
                (V3::new(centro.x + a, piso, centro.z - lado), V3::new(centro.x + a, piso, centro.z + lado)),
                (V3::new(centro.x - lado, piso, centro.z + a), V3::new(centro.x + lado, piso, centro.z + a)),
            ] {
                if let Some((a, b)) = proy.segmento(p0, p1, cerca) {
                    painter.line_segment(
                        [Pos2::new(a.0, a.1), Pos2::new(b.0, b.1)],
                        Stroke::new(1.0, pal.rejilla),
                    );
                }
            }
        }

        // --- Aristas ---
        for &(a, b) in &self.datos.aristas {
            let (Some((_, za)), Some((_, zb))) = (pant[a], pant[b]) else { continue };
            if self.ajustes.arbol {
                let on = foco.is_some() && resaltado(a) && resaltado(b);
                let principal = self.padre[b] == Some(a);
                // Las aristas extra (un archivo tocado por varias sesiones) solo al resaltar.
                if !principal && !on {
                    continue;
                }
                let tipo_padre = self.datos.nodos[a].tipo;
                let (grosor, base_col) = match tipo_padre {
                    Tipo::Raiz => (4.0, TRONCO),
                    Tipo::Carpeta => (3.0, TRONCO),
                    Tipo::Proyecto => (2.0, mezclar(TRONCO, 0.6, color(Tipo::Sesion))),
                    _ => (1.0, color(self.datos.nodos[b].tipo)),
                };
                let base = if on { 0.95 } else if foco.is_some() { 0.12 } else { 0.55 };
                let k = base * niebla((za + zb) * 0.5);
                // Rama curva: sale vertical del padre y llega vertical al hijo.
                let (p0, p3) = (disp.pos[a], disp.pos[b]);
                let dy = (p3.y - p0.y) * 0.55;
                let (p1, p2) = (p0 + V3::new(0.0, dy, 0.0), p3 - V3::new(0.0, dy, 0.0));
                let puntos: Option<Vec<Pos2>> = (0..=12)
                    .map(|j| {
                        let t = j as f32 / 12.0;
                        let u = 1.0 - t;
                        let p = p0 * (u * u * u) + p1 * (3.0 * u * u * t) + p2 * (3.0 * u * t * t) + p3 * (t * t * t);
                        proy.proyectar(p).filter(|q| q.2 > cerca).map(|q| Pos2::new(q.0, q.1))
                    })
                    .collect();
                if let Some(puntos) = puntos {
                    let escala = (proy.focal / ((za + zb) * 0.5)).clamp(0.3, 2.5);
                    painter.add(egui::Shape::line(
                        puntos,
                        Stroke::new((grosor * escala).max(0.8) + if on { 0.8 } else { 0.0 }, mezclar(base_col, k, fondo)),
                    ));
                }
                continue;
            }
            let Some((pa, pb)) = proy.segmento(disp.pos[a], disp.pos[b], cerca) else { continue };
            let (pa, pb) = (Pos2::new(pa.0, pa.1), Pos2::new(pb.0, pb.1));
            let on = foco.is_some() && resaltado(a) && resaltado(b);
            let base = if on { 0.85 } else if foco.is_some() { 0.08 } else { 0.32 };
            let k = base * niebla((za + zb) * 0.5);
            let c = mezclar(color(self.datos.nodos[b].tipo), k, fondo);
            painter.line_segment([pa, pb], Stroke::new(if on { 1.6 } else { 1.0 }, c));
        }

        // --- Nodos (de atrás hacia adelante) ---
        dibs.sort_by(|a, b| b.z.total_cmp(&a.z));
        let t = ui.input(|i| i.time) as f32;
        let ahora = Local::now();
        let mut hay_activas = false;
        // Las figuras miran hacia la cámara (un poco giradas para que se vea su grosor).
        let (sg, cg) = (self.camara.guinada + 0.45).sin_cos();
        let (sp, cp) = (-self.camara.cabeceo * 0.6).sin_cos();
        let rotar = |v: V3| {
            let y = v.y * cp - v.z * sp;
            let z = v.y * sp + v.z * cp;
            V3::new(v.x * cg + z * sg, y, -v.x * sg + z * cg)
        };
        let luz = (proy.arriba * 0.8 - proy.derecha * 0.5 - proy.adelante * 0.7).normal();
        for d in &dibs {
            let n = &self.datos.nodos[d.i];
            let mut k = niebla(d.z);
            if !resaltado(d.i) {
                k *= 0.18;
            }
            // Lo reciente brilla más.
            if let (Some(f), Tipo::Archivo | Tipo::Sesion) = (n.fecha, n.tipo) {
                let dias = (ahora - f).num_hours() as f32 / 24.0;
                k *= (1.0 - dias / 60.0).clamp(0.55, 1.0);
            }
            let c = color(n.tipo);
            let r = d.r;
            if n.activa {
                hay_activas = true;
                let pulso = 1.0 + 0.35 * (t * 3.0).sin().abs();
                painter.circle_stroke(
                    d.p,
                    r * 1.6 * pulso,
                    Stroke::new(2.0, mezclar(pal.activa, k, fondo)),
                );
            }
            // Objeto 3D de caras planas: se descartan las de espaldas, se ordenan
            // de atrás hacia adelante y cada una se ilumina según su orientación.
            if r < 2.5 {
                painter.circle_filled(d.p, r, mezclar(c, k, fondo));
            } else {
                let escala = radio_base(n.tipo) * n.peso * 1.6 * self.ajustes.tam_figuras;
                let centro = disp.pos[d.i];
                let mut caras: Vec<(f32, Vec<Pos2>, Color32)> = Vec::new();
                for f in figuras::figura(n.tipo) {
                    let normal = rotar(f.normal);
                    let pts3: Vec<V3> = f.v.iter().map(|&v| centro + rotar(v) * escala).collect();
                    let cc = pts3.iter().fold(V3::CERO, |a, &b| a + b) * (1.0 / pts3.len() as f32);
                    if normal.punto(cc - proy.ojo) >= 0.0 {
                        continue;
                    }
                    let pts: Option<Vec<Pos2>> = pts3
                        .iter()
                        .map(|&p| proy.proyectar(p).map(|(x, y, _, _)| Pos2::new(x, y)))
                        .collect();
                    let Some(pts) = pts else { continue };
                    let brillo = 0.4 + 0.6 * normal.punto(luz).max(0.0);
                    let col = mezclar(escalar(c, f.tono * brillo), k, fondo);
                    caras.push(((cc - proy.ojo).punto(proy.adelante), pts, col));
                }
                caras.sort_by(|a, b| b.0.total_cmp(&a.0));
                for (_, pts, col) in caras {
                    let mut malla = egui::Mesh::default();
                    for p in &pts {
                        malla.colored_vertex(*p, col);
                    }
                    for j in 1..pts.len() as u32 - 1 {
                        malla.add_triangle(0, j, j + 1);
                    }
                    painter.add(egui::Shape::mesh(malla));
                    // Contorno solo en caras «gruesas» en pantalla: en una cara casi de perfil
                    // los ángulos agudos hacen que el trazo dibuje picos larguísimos.
                    if r > 6.0 && !es_astilla(&pts) {
                        painter.add(egui::Shape::closed_line(
                            pts,
                            Stroke::new(0.7, mezclar(col, 0.55, fondo)),
                        ));
                    }
                }
            }
            if Some(d.i) == self.seleccion {
                painter.circle_stroke(d.p, r + 4.0, Stroke::new(1.5, pal.seleccion));
            } else if !q.is_empty() && self.coincide(d.i, q) {
                painter.circle_stroke(d.p, r + 3.0, Stroke::new(1.5, pal.coincidencia));
            }

            let etiqueta_fija = matches!(n.tipo, Tipo::Raiz | Tipo::Carpeta | Tipo::Proyecto);
            let mostrar = Some(d.i) == hover
                || Some(d.i) == self.seleccion
                || (self.ajustes.etiquetas
                    && resaltado(d.i)
                    && (etiqueta_fija || foco.is_some() || r > 14.0));
            if mostrar {
                let base = self.ajustes.tam_letra;
                let tam = if etiqueta_fija { base * 0.95 } else { base * 0.82 };
                painter.text(
                    d.p + Vec2::new(r + 5.0, 0.0),
                    Align2::LEFT_CENTER,
                    datos::recortar(&n.etiqueta, 40),
                    FontId::proportional(tam),
                    mezclar(pal.etiqueta, k.max(0.5), fondo),
                );
            }
        }

        // --- Información flotante ---
        if let (Some(h), Some(m)) = (hover, raton) {
            let n = &self.datos.nodos[h];
            let mut lineas = vec![format!("{} — {}", n.tipo.nombre(), n.etiqueta)];
            if let Some(f) = n.fecha {
                lineas.push(format!("{} ({})", f.format("%Y-%m-%d %H:%M"), hace(f)));
            }
            for (k, v) in n.detalle.iter().take(4) {
                lineas.push(format!("{k}: {}", datos::recortar(v, 70)));
            }
            let galley = painter.layout_no_wrap(
                lineas.join("\n"),
                FontId::proportional(self.ajustes.tam_letra * 0.9),
                pal.globo_texto,
            );
            let mut pos = m + Vec2::new(16.0, 16.0);
            let tam = galley.size() + Vec2::splat(16.0);
            if pos.x + tam.x > rect.right() {
                pos.x = m.x - tam.x - 10.0;
            }
            if pos.y + tam.y > rect.bottom() {
                pos.y = m.y - tam.y - 10.0;
            }
            let caja = Rect::from_min_size(pos, tam);
            painter.rect_filled(
                caja,
                CornerRadius::same(6),
                pal.globo_fondo,
            );
            painter.rect_stroke(
                caja,
                CornerRadius::same(6),
                Stroke::new(1.0, mezclar(color(n.tipo), 0.7, fondo)),
                egui::StrokeKind::Inside,
            );
            painter.galley(pos + Vec2::splat(8.0), galley, pal.globo_texto);
        }

        // --- Ayuda ---
        let visibles_n = visibles.iter().filter(|v| **v).count();
        painter.text(
            rect.left_bottom() + Vec2::new(10.0, -10.0),
            Align2::LEFT_BOTTOM,
            format!(
                "{visibles_n} nodos · arrastrar: girar / mover nodo · clic derecho: menú (ver solo una rama) · arrastrar con derecho: desplazar · rueda: zoom · doble clic: enfocar"
            ),
            FontId::proportional(self.ajustes.tam_letra * 0.8),
            pal.ayuda,
        );

        if moviendo || hay_activas || resp.dragged() || hover.is_some() {
            ui.ctx().request_repaint();
        }
    }
}

enum Accion {
    Rama(usize),
    Todo,
    Seleccionar(usize),
    Abrir(usize),
    Centrar,
}

/// Fondos propuestos para la vista 3D.
const FONDOS: [([u8; 3], &str); 6] = [
    ([13, 15, 21], "Noche"),
    ([0, 0, 0], "Negro"),
    ([24, 32, 48], "Azul profundo"),
    ([38, 40, 44], "Grafito"),
    ([238, 234, 224], "Papel"),
    ([255, 255, 255], "Blanco"),
];

/// Comando para instalar o actualizar desde GitHub.
const COMANDO_INSTALAR: &str = "curl -fsSL https://raw.githubusercontent.com/airylp/grafoC/main/install.sh | sh";

impl App {
    /// Menú del clic derecho: sobre un nodo, ver solo su rama; siempre, volver a todo.
    fn menu_contextual(&mut self, resp: &egui::Response) {
        let mut accion = None;
        let nodo = self.menu_nodo.filter(|&i| i < self.datos.nodos.len());
        let en_rama = self.rama.is_some();
        resp.context_menu(|ui| {
            if let Some(i) = nodo {
                let n = &self.datos.nodos[i];
                ui.label(RichText::new(format!("{} · {}", n.tipo.nombre(), datos::recortar(&n.etiqueta, 40))).small().weak());
                let tiene_hijos = self.padre.iter().any(|&p| p == Some(i));
                if tiene_hijos && self.rama != Some(i) && ui.button("🔍 Ver solo esta rama").clicked() {
                    accion = Some(Accion::Rama(i));
                }
                if ui.button("ℹ Ver detalle").clicked() {
                    accion = Some(Accion::Seleccionar(i));
                }
                if n.ruta.is_some() && ui.button("📂 Abrir").clicked() {
                    accion = Some(Accion::Abrir(i));
                }
                ui.separator();
            }
            if en_rama && ui.button("↩ Ver todo").clicked() {
                accion = Some(Accion::Todo);
            }
            if ui.button("⌖ Centrar").clicked() {
                accion = Some(Accion::Centrar);
            }
        });
        match accion {
            Some(Accion::Rama(i)) => {
                self.seleccion = None;
                self.ver_rama(Some(i));
            }
            Some(Accion::Todo) => self.ver_rama(None),
            Some(Accion::Seleccionar(i)) => self.seleccion = Some(i),
            Some(Accion::Abrir(i)) => {
                if let Some(p) = &self.datos.nodos[i].ruta {
                    abrir(p);
                }
            }
            Some(Accion::Centrar) => self.centrar_camara(),
            None => {}
        }
    }

    /// Aplica tema y letra cuando cambian (reconstruir las fuentes es caro).
    fn aplicar_ajustes(&mut self, ctx: &egui::Context) {
        if ctx.options(|o| o.theme_preference) != self.ajustes.tema {
            ctx.set_theme(self.ajustes.tema);
        }
        let letra = (self.ajustes.letra.clone(), self.ajustes.tam_letra);
        if self.letra_aplicada.0 != letra.0 {
            ctx.set_fonts(letras::definiciones(&letra.0));
        }
        if self.letra_aplicada.1 != letra.1 {
            letras::aplicar_tamanos(ctx, letra.1);
        }
        self.letra_aplicada = letra;
    }

    /// Guarda los ajustes al cambiar (no mientras se arrastra un control).
    fn guardar_si_cambio(&mut self, ctx: &egui::Context) {
        if self.ajustes == self.guardados || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.error_ajustes = self.ajustes.guardar().err();
        self.guardados = self.ajustes.clone();
    }

    fn ventana_ajustes(&mut self, ctx: &egui::Context) {
        if !self.ver_ajustes {
            return;
        }
        let mut abierta = true;
        let mut recentrar = false;
        let mut releer = false;
        let mut restablecer = false;
        egui::Window::new("Ajustes")
            .open(&mut abierta)
            .collapsible(false)
            .resizable(false)
            .default_width(360.0)
            .default_pos(Pos2::new(340.0, 40.0))
            .show(ctx, |ui| {
                let fondo_actual = {
                    let c = self.paleta_vista(ui.visuals().dark_mode).0.fondo;
                    [c.r(), c.g(), c.b()]
                };
                let a = &mut self.ajustes;
                ui.label(RichText::new("Apariencia").strong());
                egui::Grid::new("ajustes_apariencia").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Tema");
                    ui.horizontal(|ui| {
                        use egui::ThemePreference as T;
                        ui.selectable_value(&mut a.tema, T::Dark, "🌙 Oscuro");
                        ui.selectable_value(&mut a.tema, T::Light, "☀ Claro");
                        ui.selectable_value(&mut a.tema, T::System, "💻 Sistema");
                    });
                    ui.end_row();
                    ui.label("Tipo de letra");
                    egui::ComboBox::from_id_salt("letra")
                        .selected_text(a.letra.as_str())
                        .width(190.0)
                        .show_ui(ui, |ui| {
                            for &nombre in &self.letras_disponibles {
                                ui.selectable_value(&mut a.letra, nombre.to_string(), nombre);
                            }
                        });
                    ui.end_row();
                    ui.label("Tamaño de letra");
                    ui.add(egui::Slider::new(&mut a.tam_letra, 10.0..=16.0).step_by(0.5));
                    ui.end_row();
                    ui.label("Fondo de la vista");
                    ui.horizontal(|ui| {
                        if ui.selectable_label(a.fondo.is_none(), "Del tema").clicked() {
                            a.fondo = None;
                        }
                        let mut c = a.fondo.unwrap_or(fondo_actual);
                        if ui.color_edit_button_srgb(&mut c).on_hover_text("Elegir color").changed() {
                            a.fondo = Some(c);
                        }
                        for (col, nombre) in FONDOS {
                            let (r, resp) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::click());
                            let elegido = a.fondo == Some(col);
                            ui.painter().rect_filled(r, CornerRadius::same(3), Color32::from_rgb(col[0], col[1], col[2]));
                            let borde = if elegido { ui.visuals().selection.stroke } else { ui.visuals().widgets.inactive.bg_stroke };
                            ui.painter().rect_stroke(r, CornerRadius::same(3), Stroke::new(borde.width.max(1.0), borde.color), egui::StrokeKind::Outside);
                            if resp.on_hover_text(nombre).clicked() {
                                a.fondo = Some(col);
                            }
                        }
                    });
                    ui.end_row();
                });
                ui.separator();
                ui.label(RichText::new("Vista 3D").strong());
                egui::Grid::new("ajustes_vista").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                    ui.label("Forma");
                    ui.horizontal(|ui| {
                        let ant = a.arbol;
                        ui.selectable_value(&mut a.arbol, true, "🌳 Árbol");
                        ui.selectable_value(&mut a.arbol, false, "✦ Fuerzas");
                        recentrar = ant != a.arbol;
                    });
                    ui.end_row();
                    ui.label("Tamaño de las figuras");
                    ui.add(egui::Slider::new(&mut a.tam_figuras, 0.5..=2.0).step_by(0.05));
                    ui.end_row();
                    ui.label("Giro automático");
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut a.giro_auto, "");
                        ui.add_enabled(a.giro_auto, egui::Slider::new(&mut a.vel_giro, 0.02..=0.6).text("velocidad"));
                    });
                    ui.end_row();
                });
                ui.checkbox(&mut a.etiquetas, "Mostrar etiquetas");
                ui.checkbox(&mut a.rejilla, "Rejilla del suelo");
                ui.checkbox(&mut a.niebla, "Atenuar lo lejano");
                ui.separator();
                ui.label(RichText::new("Datos").strong());
                ui.label("Carpeta de Claude");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut a.carpeta)
                            .hint_text(datos::ruta_corta(&datos::dir_claude()))
                            .desired_width(230.0),
                    );
                    let distinta = a.carpeta_claude() != self.carpeta_leida;
                    if ui.add_enabled(distinta && self.rx.is_none(), egui::Button::new("Leer")).clicked() {
                        releer = true;
                    }
                });
                if !a.carpeta_claude().is_dir() {
                    ui.colored_label(ui.visuals().warn_fg_color, "Esa carpeta no existe.");
                }
                ui.label(RichText::new("Vacía = CLAUDE_CONFIG_DIR o ~/.claude").small().weak());
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Restablecer valores").clicked() {
                        restablecer = true;
                    }
                    if let Some(e) = &self.error_ajustes {
                        ui.colored_label(ui.visuals().error_fg_color, format!("No se guardaron: {e}"));
                    }
                });
                ui.add_space(4.0);
                let commit = option_env!("GRAFO_COMMIT").filter(|c| !c.is_empty());
                ui.label(
                    RichText::new(format!(
                        "grafo {}{}",
                        env!("CARGO_PKG_VERSION"),
                        commit.map(|c| format!(" ({c})")).unwrap_or_default()
                    ))
                    .small()
                    .weak(),
                );
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Actualizar: vuelve a ejecutar el instalador").small().weak());
                    if ui.small_button("Copiar comando").clicked() {
                        ui.ctx().copy_text(COMANDO_INSTALAR.into());
                    }
                });
                if let Some(c) = crate::ajustes::carpeta_config() {
                    ui.label(RichText::new(format!("Ajustes en {}", datos::ruta_corta(&c))).small().weak());
                }
            });
        if restablecer {
            let idioma = self.ajustes.idioma.take();
            recentrar = self.ajustes.arbol != Ajustes::default().arbol;
            releer = !self.ajustes.carpeta.is_empty();
            self.ajustes = Ajustes { idioma, ..Ajustes::default() };
        }
        if recentrar {
            self.centrar_camara();
        }
        if releer && self.rx.is_none() {
            self.recargar();
        }
        self.ver_ajustes = abierta;
    }
}
