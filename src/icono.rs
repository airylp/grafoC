//! Ícono del programa (un arbolito de nodos sobre un cuadrado redondeado oscuro),
//! generado en código a cualquier tamaño, y escritura de PNG sin dependencias.

use std::io::Write;
use std::path::Path;

type Rgb = [f32; 3];

/// Forma del ícono en coordenadas 0..1 (y hacia abajo).
enum Forma {
    /// Segmento con extremos redondeados: a, b, grosor.
    Rama((f32, f32), (f32, f32), f32, Rgb),
    Circulo((f32, f32), f32, Rgb),
}

fn formas() -> Vec<Forma> {
    let tronco = [0.62, 0.45, 0.30];
    let raiz = (0.5, 0.80);
    let medio = (0.5, 0.58);
    let ramas = [(0.30, 0.42), (0.5, 0.36), (0.70, 0.42)];
    let hojas = [
        ((0.30, 0.42), [(0.20, 0.28), (0.33, 0.24)], [1.0, 0.65, 0.29]),
        ((0.5, 0.36), [(0.44, 0.20), (0.57, 0.20)], [0.73, 0.51, 1.0]),
        ((0.70, 0.42), [(0.67, 0.24), (0.80, 0.28)], [1.0, 0.33, 0.43]),
    ];
    let mut f = vec![Forma::Rama(raiz, medio, 0.05, tronco)];
    for &r in &ramas {
        f.push(Forma::Rama(medio, r, 0.032, tronco));
    }
    for (desde, puntas, color) in hojas {
        for p in puntas {
            f.push(Forma::Rama(desde, p, 0.016, [0.55, 0.5, 0.5]));
            f.push(Forma::Circulo(p, 0.045, color));
        }
    }
    for &r in &ramas {
        f.push(Forma::Circulo(r, 0.05, [0.31, 0.86, 0.63]));
    }
    f.push(Forma::Circulo(raiz, 0.065, [1.0, 0.84, 0.47]));
    f
}

fn dist_segmento(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (abx, aby) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * abx + (p.1 - a.1) * aby) / (abx * abx + aby * aby)).clamp(0.0, 1.0);
    let (dx, dy) = (p.0 - a.0 - abx * t, p.1 - a.1 - aby * t);
    (dx * dx + dy * dy).sqrt()
}

/// Imagen RGBA de `n`×`n`. Deja el margen que macOS espera alrededor del cuadrado.
pub fn rgba(n: u32) -> Vec<u8> {
    let formas = formas();
    let mut px = vec![0u8; (n * n * 4) as usize];
    const SUB: u32 = 4;
    let margen = 0.098;
    let lado = 1.0 - 2.0 * margen;
    let radio = lado * 0.225;
    for y in 0..n {
        for x in 0..n {
            let mut acum = [0.0f32; 4];
            for sy in 0..SUB {
                for sx in 0..SUB {
                    let u = (x as f32 + (sx as f32 + 0.5) / SUB as f32) / n as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / SUB as f32) / n as f32;
                    // Cuadrado redondeado.
                    let qx = ((u - 0.5).abs() - (lado * 0.5 - radio)).max(0.0);
                    let qy = ((v - 0.5).abs() - (lado * 0.5 - radio)).max(0.0);
                    if (qx * qx + qy * qy).sqrt() > radio {
                        continue;
                    }
                    let k = (v - margen) / lado;
                    let mut c: Rgb = [0.09 - 0.04 * k, 0.11 - 0.05 * k, 0.17 - 0.07 * k];
                    for f in &formas {
                        let (dentro, col) = match f {
                            Forma::Rama(a, b, g, col) => (dist_segmento((u, v), *a, *b) <= g * 0.5, col),
                            Forma::Circulo(centro, r, col) => {
                                let d = ((u - centro.0).powi(2) + (v - centro.1).powi(2)).sqrt();
                                (d <= *r, col)
                            }
                        };
                        if dentro {
                            c = *col;
                        }
                    }
                    acum[0] += c[0];
                    acum[1] += c[1];
                    acum[2] += c[2];
                    acum[3] += 1.0;
                }
            }
            if acum[3] > 0.0 {
                let i = ((y * n + x) * 4) as usize;
                for k in 0..3 {
                    px[i + k] = (acum[k] / acum[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                px[i + 3] = (acum[3] / (SUB * SUB) as f32 * 255.0).round() as u8;
            }
        }
    }
    px
}

fn crc32(datos: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &b in datos {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

/// PNG RGBA sin compresión (bloques «stored» de deflate).
pub fn png(n: u32, rgba: &[u8]) -> Vec<u8> {
    let mut crudo = Vec::with_capacity(rgba.len() + n as usize);
    for fila in rgba.chunks(n as usize * 4) {
        crudo.push(0);
        crudo.extend_from_slice(fila);
    }
    let mut z = vec![0x78, 0x01];
    let trozos: Vec<&[u8]> = crudo.chunks(65_535).collect();
    for (i, t) in trozos.iter().enumerate() {
        z.push((i + 1 == trozos.len()) as u8);
        let l = t.len() as u16;
        z.extend_from_slice(&l.to_le_bytes());
        z.extend_from_slice(&(!l).to_le_bytes());
        z.extend_from_slice(t);
    }
    let (mut a, mut b) = (1u32, 0u32);
    for &x in &crudo {
        a = (a + x as u32) % 65_521;
        b = (b + a) % 65_521;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut trozo = |tipo: &[u8], datos: &[u8]| {
        out.extend_from_slice(&(datos.len() as u32).to_be_bytes());
        let mut c = tipo.to_vec();
        c.extend_from_slice(datos);
        out.extend_from_slice(&c);
        out.extend_from_slice(&crc32(&c).to_be_bytes());
    };
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&n.to_be_bytes());
    ihdr.extend_from_slice(&n.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    trozo(b"IHDR", &ihdr);
    trozo(b"IDAT", &z);
    trozo(b"IEND", &[]);
    out
}

/// Carpeta `.iconset` para `iconutil` (macOS) con todos los tamaños.
pub fn escribir_iconset(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    for base in [16u32, 32, 128, 256, 512] {
        for escala in [1u32, 2] {
            let n = base * escala;
            let nombre = if escala == 1 { format!("icon_{base}x{base}.png") } else { format!("icon_{base}x{base}@2x.png") };
            std::fs::File::create(dir.join(nombre))?.write_all(&png(n, &rgba(n)))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod pruebas {
    #[test]
    fn png_valido() {
        let p = super::png(4, &super::rgba(4));
        assert!(p.starts_with(b"\x89PNG"));
        assert_eq!(super::crc32(b"IEND"), 0xAE42_6082);
    }
}
