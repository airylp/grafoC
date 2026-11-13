//! Objetos 3D de caras planas que representan cada tipo de nodo.
//!
//! Cada figura está en coordenadas locales de aprox. [-1, 1], con el frente hacia +z.

use std::sync::OnceLock;

use crate::datos::Tipo;
use crate::espacio::V3;

pub struct Cara {
    pub v: Vec<V3>,
    pub normal: V3,
    /// Multiplicador de color: 1 normal, >1 más claro, <1 más oscuro.
    pub tono: f32,
}

/// Prisma: polígono convexo (x, y) extruido entre z0 y z1.
fn extruir(poly: &[(f32, f32)], z0: f32, z1: f32, tono: f32, tono_frente: f32) -> Vec<Cara> {
    let mut caras = Vec::new();
    let frente: Vec<V3> = poly.iter().map(|&(x, y)| V3::new(x, y, z1)).collect();
    let atras: Vec<V3> = poly.iter().rev().map(|&(x, y)| V3::new(x, y, z0)).collect();
    caras.push(cara(frente, tono_frente));
    caras.push(cara(atras, tono * 0.9));
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        caras.push(cara(
            vec![
                V3::new(a.0, a.1, z0),
                V3::new(b.0, b.1, z0),
                V3::new(b.0, b.1, z1),
                V3::new(a.0, a.1, z1),
            ],
            tono,
        ));
    }
    orientar_hacia_afuera(&mut caras);
    caras
}

fn cara(v: Vec<V3>, tono: f32) -> Cara {
    let normal = (v[1] - v[0]).cruz(v[2] - v[0]).normal();
    Cara { v, normal, tono }
}

/// Garantiza que las normales de una pieza convexa apunten hacia afuera.
fn orientar_hacia_afuera(caras: &mut [Cara]) {
    let todos: Vec<V3> = caras.iter().flat_map(|c| c.v.iter().copied()).collect();
    let centro = todos.iter().fold(V3::CERO, |a, &b| a + b) * (1.0 / todos.len() as f32);
    for c in caras {
        let cc = c.v.iter().fold(V3::CERO, |a, &b| a + b) * (1.0 / c.v.len() as f32);
        if c.normal.punto(cc - centro) < 0.0 {
            c.v.reverse();
            c.normal = c.normal * -1.0;
        }
    }
}

fn gema() -> Vec<Cara> {
    let p = [
        V3::new(1.0, 0.0, 0.0),
        V3::new(-1.0, 0.0, 0.0),
        V3::new(0.0, 1.25, 0.0),
        V3::new(0.0, -1.25, 0.0),
        V3::new(0.0, 0.0, 1.0),
        V3::new(0.0, 0.0, -1.0),
    ];
    let mut caras = Vec::new();
    for &y in &[2, 3] {
        for &(a, b) in &[(0, 4), (4, 1), (1, 5), (5, 0)] {
            let tono = if y == 2 { 1.15 } else { 0.85 };
            caras.push(cara(vec![p[a], p[b], p[y]], tono));
        }
    }
    orientar_hacia_afuera(&mut caras);
    caras
}

fn carpeta() -> Vec<Cara> {
    let mut c = extruir(&[(-1.0, -0.7), (1.0, -0.7), (1.0, 0.5), (-1.0, 0.5)], -0.16, 0.16, 1.0, 1.0);
    c.extend(extruir(
        &[(-1.0, 0.45), (-0.1, 0.45), (-0.25, 0.75), (-0.92, 0.75)],
        -0.16,
        0.16,
        1.25,
        1.25,
    ));
    c
}

fn caja() -> Vec<Cara> {
    let mut c = extruir(&[(-0.85, -0.85), (0.85, -0.85), (0.85, 0.85), (-0.85, 0.85)], -0.85, 0.85, 1.0, 1.0);
    // La tapa (cara superior) más clara, como una caja de cartón.
    for f in &mut c {
        if f.normal.y > 0.9 {
            f.tono = 1.4;
        }
    }
    // Cinta sobre el frente.
    c.extend(extruir(&[(-0.12, -0.85), (0.12, -0.85), (0.12, 0.85), (-0.12, 0.85)], 0.85, 0.9, 0.7, 0.7));
    c
}

fn globo() -> Vec<Cara> {
    let mut c = extruir(
        &[
            (-0.75, -0.5),
            (0.75, -0.5),
            (1.0, -0.25),
            (1.0, 0.45),
            (0.75, 0.7),
            (-0.75, 0.7),
            (-1.0, 0.45),
            (-1.0, -0.25),
        ],
        -0.2,
        0.2,
        1.0,
        1.05,
    );
    c.extend(extruir(&[(-0.6, -0.48), (-0.15, -0.48), (-0.75, -1.0)], -0.2, 0.2, 1.0, 1.05));
    // Tres puntos de «escribiendo…».
    for x in [-0.45f32, 0.0, 0.45] {
        c.extend(extruir(
            &[(x - 0.1, 0.0), (x + 0.1, 0.0), (x + 0.1, 0.2), (x - 0.1, 0.2)],
            0.2,
            0.26,
            1.5,
            1.5,
        ));
    }
    c
}

fn hoja() -> Vec<Cara> {
    let mut c = extruir(
        &[(-0.72, -1.0), (0.72, -1.0), (0.72, 0.55), (0.27, 1.0), (-0.72, 1.0)],
        -0.06,
        0.06,
        0.95,
        1.15,
    );
    // Esquina doblada.
    c.extend(extruir(&[(0.27, 0.55), (0.72, 0.55), (0.27, 1.0)], 0.06, 0.1, 0.8, 0.8));
    // Renglones de texto.
    for (y, ancho) in [(0.3f32, 0.75f32), (0.0, 1.0), (-0.3, 1.0), (-0.6, 0.6)] {
        let x1 = -0.5 + ancho;
        c.extend(extruir(&[(-0.5, y), (x1, y), (x1, y + 0.09), (-0.5, y + 0.09)], 0.06, 0.09, 0.55, 0.55));
    }
    c
}

fn bandera() -> Vec<Cara> {
    // Mástil oscuro y bandera triangular.
    let mut c = extruir(&[(-0.78, -1.1), (-0.62, -1.1), (-0.62, 1.05), (-0.78, 1.05)], -0.08, 0.08, 0.45, 0.45);
    c.extend(extruir(&[(-0.62, 1.05), (0.95, 0.6), (-0.62, 0.12)], -0.06, 0.06, 1.0, 1.1));
    // Base.
    c.extend(extruir(&[(-1.0, -1.2), (-0.4, -1.2), (-0.4, -1.08), (-1.0, -1.08)], -0.3, 0.3, 0.5, 0.5));
    c
}

pub fn figura(t: Tipo) -> &'static [Cara] {
    static FIGURAS: OnceLock<Vec<Vec<Cara>>> = OnceLock::new();
    let todas = FIGURAS.get_or_init(|| {
        Tipo::TODOS
            .iter()
            .map(|t| match t {
                Tipo::Raiz => gema(),
                Tipo::Carpeta => carpeta(),
                Tipo::Proyecto => caja(),
                Tipo::Sesion => globo(),
                Tipo::Archivo => hoja(),
                Tipo::Pendiente => bandera(),
            })
            .collect()
    });
    &todas[Tipo::TODOS.iter().position(|&x| x == t).unwrap_or(0)]
}

#[cfg(test)]
mod pruebas {
    use super::*;

    #[test]
    fn normales_hacia_afuera_en_piezas_convexas() {
        for c in caja().iter().take(6) {
            let centro = c.v.iter().fold(V3::CERO, |a, &b| a + b) * (1.0 / c.v.len() as f32);
            assert!(c.normal.punto(centro) > 0.0);
        }
        assert_eq!(gema().len(), 8);
    }
}
