mod ajustes;
mod app;
mod datos;
mod espacio;
mod figuras;
mod icono;
mod letras;

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    // `grafo --iconset carpeta`: PNG del ícono para empaquetar (macOS / Linux).
    if let Some(i) = args.iter().position(|a| a == "--iconset") {
        let dir = args.get(i + 1).map(String::as_str).unwrap_or("grafo.iconset");
        if let Err(e) = icono::escribir_iconset(std::path::Path::new(dir)) {
            eprintln!("no se pudo escribir el ícono: {e}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if std::env::args().any(|a| a == "--disposicion") {
        disposicion();
        return Ok(());
    }
    if std::env::args().any(|a| a == "--resumen") {
        resumen();
        return Ok(());
    }
    let opciones = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Grafo de Claude")
            .with_inner_size([1400.0, 860.0])
            .with_min_inner_size([800.0, 500.0])
            .with_icon(eframe::egui::IconData { rgba: icono::rgba(256), width: 256, height: 256 }),
        ..Default::default()
    };
    eframe::run_native(
        "Grafo de Claude",
        opciones,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// Carga los datos sin abrir ventana e imprime un resumen (útil para depurar).
fn resumen() {
    let t = std::time::Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || datos::cargar(datos::dir_claude(), tx));
    for m in rx {
        if let datos::Mensaje::Listo(d) = m {
            for t in datos::Tipo::TODOS {
                let n = d.nodos.iter().filter(|n| n.tipo == t).count();
                println!("{:<22} {n}", t.nombre());
            }
            println!("Aristas                {}", d.aristas.len());
            println!("\nÚltimos archivos modificados:");
            for &i in d.recientes.iter().take(8) {
                let n = &d.nodos[i];
                println!("  {}  {}", n.fecha.map(datos::hace).unwrap_or_default(), n.etiqueta);
            }
            println!("\nPendientes (primeros 8):");
            for &i in d.pendientes.iter().take(8) {
                println!("  {}", d.nodos[i].etiqueta);
            }
        }
    }
    println!("\nCarga en {:.2?}", t.elapsed());
}

/// Simula la disposición sin ventana y muestra los nodos más alejados (depuración).
fn disposicion() {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || datos::cargar(datos::dir_claude(), tx));
    let d = rx
        .into_iter()
        .find_map(|m| if let datos::Mensaje::Listo(d) = m { Some(d) } else { None })
        .expect("datos");
    let mut disp = espacio::Disposicion::nueva(&d);
    let vis = vec![true; d.nodos.len()];
    for paso in 0..3000 {
        disp.paso(&d, &vis, None);
        if paso % 500 == 0 {
            let max = disp.pos.iter().map(|p| p.largo()).fold(0.0f32, f32::max);
            println!("paso {paso}: radio máx {max:.0}, temperatura {:.3}", disp.temperatura);
        }
    }
    // Misma cámara que la ventana capturada (área central de 1005x860).
    let mut cam = espacio::Camara::default();
    cam.distancia = 765.0 / (cam.fov * 0.5).tan() * 1.05;
    let proy = cam.proyeccion(502.0, 430.0, 860.0);
    let mut largas = 0;
    for &(a, b) in &d.aristas {
        if let (Some(pa), Some(pb)) = (proy.proyectar(disp.pos[a]), proy.proyectar(disp.pos[b])) {
            let l = ((pa.0 - pb.0).powi(2) + (pa.1 - pb.1).powi(2)).sqrt();
            if l > 400.0 {
                largas += 1;
                if largas <= 6 {
                    println!("arista {l:.0}px  {:?} {} -> {:?} {}  pos {:?} -> {:?}", d.nodos[a].tipo, d.nodos[a].etiqueta, d.nodos[b].tipo, d.nodos[b].etiqueta, disp.pos[a], disp.pos[b]);
                }
            }
        }
    }
    println!("aristas de más de 400 px: {largas} de {}", d.aristas.len());
    let nan = disp.pos.iter().filter(|p| !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite()).count();
    println!("posiciones no finitas: {nan}");
    let mut lejos: Vec<_> = (0..d.nodos.len()).collect();
    lejos.sort_by(|&a, &b| disp.pos[b].largo().total_cmp(&disp.pos[a].largo()));
    for &i in lejos.iter().take(5) {
        let p = disp.pos[i];
        println!("{:>7.0} {:>7.0} {:>7.0}  {:?} {}", p.x, p.y, p.z, d.nodos[i].tipo, d.nodos[i].etiqueta);
    }
}
