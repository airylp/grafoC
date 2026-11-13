//! Geometría 3D mínima, cámara orbital y disposición por fuerzas.

use std::ops::{Add, AddAssign, Mul, Sub, SubAssign};

use crate::datos::{Datos, Tipo};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct V3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl V3 {
    pub const CERO: V3 = V3 { x: 0.0, y: 0.0, z: 0.0 };
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        V3 { x, y, z }
    }
    pub fn punto(self, o: V3) -> f32 {
        self.x * o.x + self.y * o.y + self.z * o.z
    }
    pub fn cruz(self, o: V3) -> V3 {
        V3::new(
            self.y * o.z - self.z * o.y,
            self.z * o.x - self.x * o.z,
            self.x * o.y - self.y * o.x,
        )
    }
    pub fn largo2(self) -> f32 {
        self.punto(self)
    }
    pub fn largo(self) -> f32 {
        self.largo2().sqrt()
    }
    pub fn normal(self) -> V3 {
        let l = self.largo();
        if l > 1e-6 { self * (1.0 / l) } else { V3::new(0.0, 1.0, 0.0) }
    }
}

impl Add for V3 {
    type Output = V3;
    fn add(self, o: V3) -> V3 {
        V3::new(self.x + o.x, self.y + o.y, self.z + o.z)
    }
}
impl Sub for V3 {
    type Output = V3;
    fn sub(self, o: V3) -> V3 {
        V3::new(self.x - o.x, self.y - o.y, self.z - o.z)
    }
}
impl Mul<f32> for V3 {
    type Output = V3;
    fn mul(self, k: f32) -> V3 {
        V3::new(self.x * k, self.y * k, self.z * k)
    }
}
impl AddAssign for V3 {
    fn add_assign(&mut self, o: V3) {
        *self = *self + o;
    }
}
impl SubAssign for V3 {
    fn sub_assign(&mut self, o: V3) {
        *self = *self - o;
    }
}

// ---------------------------------------------------------------------------
// Cámara

pub struct Camara {
    pub objetivo: V3,
    pub destino: V3,
    pub guinada: f32,
    pub cabeceo: f32,
    pub distancia: f32,
    pub fov: f32,
}

pub struct Proyeccion {
    pub ojo: V3,
    pub adelante: V3,
    pub derecha: V3,
    pub arriba: V3,
    pub focal: f32,
    cx: f32,
    cy: f32,
}

impl Default for Camara {
    fn default() -> Self {
        Camara {
            objetivo: V3::CERO,
            destino: V3::CERO,
            guinada: 0.6,
            cabeceo: 0.62,
            distancia: 900.0,
            fov: 55f32.to_radians(),
        }
    }
}

impl Camara {
    /// Vista casi de frente, ligeramente desde arriba: para el árbol.
    pub fn de_frente() -> Self {
        Camara { guinada: 0.0, cabeceo: 0.16, ..Camara::default() }
    }

    pub fn ojo(&self) -> V3 {
        let (sy, cy) = self.guinada.sin_cos();
        let (sp, cp) = self.cabeceo.sin_cos();
        self.objetivo + V3::new(cp * sy, sp, cp * cy) * self.distancia
    }

    pub fn rotar(&mut self, dx: f32, dy: f32) {
        self.guinada -= dx * 0.008;
        self.cabeceo = (self.cabeceo + dy * 0.008).clamp(-1.5, 1.5);
    }

    pub fn desplazar(&mut self, dx: f32, dy: f32, alto: f32) {
        let p = self.proyeccion(0.0, 0.0, alto);
        let k = self.distancia / p.focal;
        let d = p.derecha * (-dx * k) + p.arriba * (dy * k);
        self.objetivo += d;
        self.destino += d;
    }

    pub fn acercar(&mut self, factor: f32) {
        self.distancia = (self.distancia * factor).clamp(20.0, 20000.0);
    }

    /// Mueve el objetivo suavemente hacia `destino`.
    pub fn animar(&mut self, dt: f32) -> bool {
        let d = self.destino - self.objetivo;
        if d.largo2() < 0.01 {
            return false;
        }
        self.objetivo += d * (1.0 - (-dt * 6.0).exp());
        true
    }

    pub fn proyeccion(&self, cx: f32, cy: f32, alto: f32) -> Proyeccion {
        let ojo = self.ojo();
        let adelante = (self.objetivo - ojo).normal();
        let derecha = adelante.cruz(V3::new(0.0, 1.0, 0.0)).normal();
        let arriba = derecha.cruz(adelante);
        Proyeccion {
            ojo,
            adelante,
            derecha,
            arriba,
            focal: alto * 0.5 / (self.fov * 0.5).tan(),
            cx,
            cy,
        }
    }
}

impl Proyeccion {
    /// Proyecta un segmento recortándolo contra el plano cercano (`cerca` unidades delante
    /// del ojo), para que no cruce la pantalla cuando un extremo queda detrás de la cámara.
    pub fn segmento(&self, a: V3, b: V3, cerca: f32) -> Option<((f32, f32), (f32, f32))> {
        let za = (a - self.ojo).punto(self.adelante);
        let zb = (b - self.ojo).punto(self.adelante);
        if za < cerca && zb < cerca {
            return None;
        }
        let corte = |p: V3, q: V3, zp: f32, zq: f32| p + (q - p) * ((cerca - zp) / (zq - zp));
        let a2 = if za < cerca { corte(a, b, za, zb) } else { a };
        let b2 = if zb < cerca { corte(b, a, zb, za) } else { b };
        let (pa, pb) = (self.proyectar(a2)?, self.proyectar(b2)?);
        Some(((pa.0, pa.1), (pb.0, pb.1)))
    }

    /// Devuelve (x, y en pantalla, profundidad, escala) o None si está detrás de la cámara.
    pub fn proyectar(&self, p: V3) -> Option<(f32, f32, f32, f32)> {
        let r = p - self.ojo;
        let z = r.punto(self.adelante);
        if z < 1.0 {
            return None;
        }
        let k = self.focal / z;
        Some((
            self.cx + r.punto(self.derecha) * k,
            self.cy - r.punto(self.arriba) * k,
            z,
            k,
        ))
    }
}

// ---------------------------------------------------------------------------
// Disposición por fuerzas

pub struct Disposicion {
    pub pos: Vec<V3>,
    vel: Vec<V3>,
    pub temperatura: f32,
    /// Con árbol, cada nodo se desliza hacia su posición fija en vez de simular fuerzas.
    pub meta: Option<Vec<V3>>,
}

fn largo_resorte(t: Tipo) -> f32 {
    match t {
        Tipo::Raiz => 0.0,
        Tipo::Carpeta => 170.0,
        Tipo::Proyecto => 220.0,
        Tipo::Sesion => 90.0,
        Tipo::Archivo => 55.0,
        Tipo::Pendiente => 75.0,
    }
}

/// Generador pseudoaleatorio determinista (para que el grafo salga igual cada vez).
fn azar(semilla: &mut u64) -> f32 {
    *semilla ^= *semilla << 13;
    *semilla ^= *semilla >> 7;
    *semilla ^= *semilla << 17;
    (*semilla % 10_000) as f32 / 10_000.0 - 0.5
}

impl Disposicion {
    pub fn nueva(d: &Datos) -> Self {
        let n = d.nodos.len();
        let mut pos = vec![V3::CERO; n];
        let mut semilla = 0x9E37_79B9_7F4A_7C15u64;
        // Cada hijo nace cerca de su padre, en una dirección aleatoria.
        for &(a, b) in &d.aristas {
            if pos[b] == V3::CERO && b != 0 {
                let dir = V3::new(azar(&mut semilla), azar(&mut semilla), azar(&mut semilla)).normal();
                let dir = V3::new(dir.x, dir.y * 0.2, dir.z).normal();
                pos[b] = pos[a] + dir * largo_resorte(d.nodos[b].tipo);
            }
        }
        Disposicion { pos, vel: vec![V3::CERO; n], temperatura: 1.0, meta: None }
    }

    pub fn reactivar(&mut self) {
        self.temperatura = self.temperatura.max(0.6);
    }

    /// Un paso de simulación sobre los nodos visibles. Devuelve true si sigue moviéndose.
    pub fn paso(&mut self, d: &Datos, visible: &[bool], fijo: Option<usize>) -> bool {
        if let Some(meta) = &self.meta {
            let mut max = 0.0f32;
            for i in 0..self.pos.len() {
                if !visible[i] || Some(i) == fijo {
                    continue;
                }
                let dlt = meta[i] - self.pos[i];
                max = max.max(dlt.largo2());
                self.pos[i] += dlt * 0.12;
            }
            return max > 0.25;
        }
        if self.temperatura < 0.02 {
            return false;
        }
        let n = self.pos.len();
        let mut fuerza = vec![V3::CERO; n];
        let idx: Vec<usize> = (0..n).filter(|&i| visible[i]).collect();

        // Repulsión entre todos los pares visibles.
        const K_REP: f32 = 2600.0;
        for (a_i, &a) in idx.iter().enumerate() {
            let pa = self.pos[a];
            for &b in &idx[a_i + 1..] {
                let mut dlt = pa - self.pos[b];
                let mut d2 = dlt.largo2();
                if d2 < 1.0 {
                    dlt = V3::new((a as f32).sin(), (b as f32).cos(), 0.3);
                    d2 = 1.0;
                }
                if d2 > 640_000.0 {
                    continue;
                }
                let f = dlt * (K_REP * d.nodos[a].peso * d.nodos[b].peso / (d2 * d2.sqrt()));
                fuerza[a] += f;
                fuerza[b] -= f;
            }
        }

        // Resortes en las aristas.
        for &(a, b) in &d.aristas {
            if !visible[a] || !visible[b] {
                continue;
            }
            let dlt = self.pos[b] - self.pos[a];
            let l = dlt.largo().max(0.01);
            let desc = l - largo_resorte(d.nodos[b].tipo);
            let f = dlt * (0.04 * desc / l);
            fuerza[a] += f * 0.5;
            fuerza[b] -= f;
        }

        // Gravedad suave hacia el origen y la raíz fija en el centro.
        let mut energia = 0.0;
        for &i in &idx {
            fuerza[i] -= self.pos[i] * 0.002;
            // Grafo casi plano: un disco con algo de relieve.
            fuerza[i].y -= self.pos[i].y * 0.06;
            if i == 0 || Some(i) == fijo {
                self.vel[i] = V3::CERO;
                if i == 0 {
                    self.pos[i] = V3::CERO;
                }
                continue;
            }
            let mut v = (self.vel[i] + fuerza[i]) * 0.82;
            let max = 40.0 * self.temperatura;
            if v.largo2() > max * max {
                v = v.normal() * max;
            }
            self.vel[i] = v;
            self.pos[i] += v * self.temperatura;
            energia += v.largo2();
        }
        let media = energia / idx.len().max(1) as f32;
        self.temperatura *= if media < 0.05 { 0.96 } else { 0.997 };
        true
    }
}

// ---------------------------------------------------------------------------
// Disposición en árbol que crece hacia arriba

fn es_hoja(t: Tipo) -> bool {
    matches!(t, Tipo::Archivo | Tipo::Pendiente)
}

fn nivel(t: Tipo) -> f32 {
    match t {
        Tipo::Raiz => 0.0,
        Tipo::Carpeta => 1.0,
        Tipo::Proyecto => 2.0,
        _ => 3.0,
    }
}

/// Separación entre los miembros de una copa y radio de una copa de `k` miembros.
const SEP_HOJA: f32 = 10.0;
fn radio_copa(k: usize, sep: f32) -> f32 {
    sep * (k as f32).sqrt() + 8.0
}
/// Distancia en profundidad entre filas de ramas.
const FILA_Z: f32 = 110.0;

#[derive(Clone, Copy)]
enum Hueco {
    /// Un hijo con descendientes: ocupa su propio ancho.
    Rama(usize),
    /// Archivos y pendientes del nodo: copa en cúpula sobre él.
    Hojas,
    /// Hijos sin descendientes (carpetas vacías, sesiones sin archivos): copa a su propio nivel.
    Brotes,
}

/// Raíz abajo, un nivel más arriba por generación (carpetas, proyectos, sesiones).
/// Cada rama recibe el ancho que necesitan sus descendientes y las más anchas van al centro.
/// Si un nodo tiene muchas ramas, se reparten en varias filas de profundidad (árbol «medio plano»).
pub fn disposicion_arbol(d: &Datos, visible: &[bool], padre: &[Option<usize>]) -> Vec<V3> {
    let n = d.nodos.len();
    let mut hijos = vec![Vec::new(); n];
    for i in 0..n {
        if let (true, Some(p)) = (visible[i], padre[i]) {
            hijos[p].push(i);
        }
    }
    let mut ramas = vec![Vec::new(); n];
    let mut hojas = vec![Vec::new(); n];
    let mut brotes = vec![Vec::new(); n];
    for p in 0..n {
        for &h in &hijos[p] {
            if es_hoja(d.nodos[h].tipo) {
                hojas[p].push(h);
            } else if hijos[h].is_empty() {
                brotes[p].push(h);
            } else {
                ramas[p].push(h);
            }
        }
    }
    let sep_brote = 26.0;
    let ancho_copa = |k: usize, sep: f32| if k == 0 { 0.0 } else { 2.0 * radio_copa(k, sep) + 18.0 };

    // Anchos y filas de abajo hacia arriba (los padres siempre tienen id menor que sus hijos).
    let mut ancho = vec![0.0f32; n];
    let mut filas: Vec<Vec<Vec<(f32, Hueco)>>> = vec![Vec::new(); n];
    for i in (0..n).rev() {
        if !visible[i] {
            continue;
        }
        let mut huecos: Vec<(f32, Hueco)> = ramas[i].iter().map(|&h| (ancho[h], Hueco::Rama(h))).collect();
        if !hojas[i].is_empty() {
            huecos.push((ancho_copa(hojas[i].len(), SEP_HOJA), Hueco::Hojas));
        }
        // Los brotes de la raíz (carpetas sin contenido) van en anillo alrededor del tronco.
        if !brotes[i].is_empty() && d.nodos[i].tipo != Tipo::Raiz {
            huecos.push((ancho_copa(brotes[i].len(), sep_brote), Hueco::Brotes));
        }
        huecos.sort_by(|a, b| b.0.total_cmp(&a.0));
        let nf = if ramas[i].len() > 6 {
            ((ramas[i].len() as f32 / 3.0).sqrt().ceil() as usize).clamp(1, 4)
        } else {
            1
        };
        // Reparto en filas equilibrando el ancho (la fila más angosta recibe el siguiente).
        let mut f: Vec<Vec<(f32, Hueco)>> = vec![Vec::new(); nf];
        let mut suma = vec![0.0f32; nf];
        for h in huecos {
            let k = (0..nf).min_by(|&a, &b| suma[a].total_cmp(&suma[b])).unwrap_or(0);
            suma[k] += h.0;
            f[k].push(h);
        }
        // Dentro de cada fila: el más ancho al centro, los demás alternando a los lados.
        for fila in &mut f {
            let mut orden = std::collections::VecDeque::new();
            for (k, h) in fila.drain(..).enumerate() {
                if k % 2 == 0 { orden.push_back(h) } else { orden.push_front(h) }
            }
            *fila = orden.into_iter().collect();
        }
        ancho[i] = suma.iter().copied().fold(0.0, f32::max).max(40.0);
        filas[i] = f;
    }

    let total = ancho[0].max(1.0);
    // Altura entre niveles proporcional al ancho, para que el árbol no quede como una franja.
    let alto = (total / 6.0).clamp(150.0, 600.0);
    let mut pos = vec![V3::CERO; n];

    let copa = |pos: &mut Vec<V3>, lista: &[usize], centro: V3, sep: f32, alto_copa: f32| {
        let rc = radio_copa(lista.len(), sep);
        for (k, &hj) in lista.iter().enumerate() {
            // Filotaxis (ángulo áureo): repartidos en un disco y abombados como una copa.
            let rad = sep * (k as f32 + 0.5).sqrt();
            let ang = k as f32 * 2.399_963;
            pos[hj] = centro + V3::new(rad * ang.cos(), (rc - rad) * alto_copa, rad * ang.sin());
        }
    };

    // Recorrido sin recursión: (nodo, centro x, z).
    let mut pila = vec![(0usize, 0.0f32, 0.0f32)];
    while let Some((i, cx, z)) = pila.pop() {
        let y = nivel(d.nodos[i].tipo) * alto;
        pos[i] = V3::new(cx, y, z);
        if d.nodos[i].tipo == Tipo::Raiz && !brotes[i].is_empty() {
            let m = brotes[i].len();
            let radio = (m as f32 * 34.0 / std::f32::consts::TAU).max(170.0);
            for (k, &b) in brotes[i].iter().enumerate() {
                let ang = k as f32 / m as f32 * std::f32::consts::TAU + 0.3;
                // Alturas alternadas para que las etiquetas no se encimen.
                let dy = if k % 2 == 0 { 0.0 } else { alto * 0.22 };
                pos[b] = V3::new(cx + radio * ang.cos(), alto * 0.75 + dy, z + radio * ang.sin());
            }
        }
        let nf = filas[i].len();
        for (r, fila) in filas[i].iter().enumerate() {
            let zf = z + (r as f32 - (nf as f32 - 1.0) * 0.5) * FILA_Z;
            let ancho_fila: f32 = fila.iter().map(|h| h.0).sum();
            let mut x = cx - ancho_fila * 0.5;
            for &(w, h) in fila {
                let centro_x = x + w * 0.5;
                match h {
                    Hueco::Rama(rm) => pila.push((rm, centro_x, zf)),
                    Hueco::Hojas => copa(
                        &mut pos,
                        &hojas[i],
                        V3::new(centro_x, y + alto * 0.55, zf),
                        SEP_HOJA,
                        0.7,
                    ),
                    Hueco::Brotes => {
                        let nivel_hijos = brotes[i]
                            .first()
                            .map(|&b| nivel(d.nodos[b].tipo))
                            .unwrap_or(0.0);
                        copa(
                            &mut pos,
                            &brotes[i],
                            V3::new(centro_x, nivel_hijos * alto, zf),
                            sep_brote,
                            0.3,
                        )
                    }
                }
                x += w;
            }
        }
    }
    // Los nodos ocultos esperan sobre su padre (así aparecen desde ahí al mostrarse).
    for i in 0..n {
        if !visible[i] {
            if let Some(p) = padre[i] {
                pos[i] = pos[p];
            }
        }
    }
    pos
}
