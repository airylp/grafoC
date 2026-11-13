# grafoC

Aplicación de escritorio (Rust + egui) que muestra en 3D tu carpeta de Claude
(`~/.claude`): carpetas, proyectos, sesiones, los últimos archivos modificados
y los pendientes.

- **Árbol que crece hacia arriba**: `~/.claude` en la base, ramas hacia los
  proyectos y sus sesiones, y copas con los archivos tocados y los pendientes.
  También hay una vista por fuerzas (✦).
- **Figuras con significado**: gema (raíz), carpeta, caja (proyecto), globo de
  diálogo (sesión), hoja de papel (archivo) y bandera (pendiente). Las sesiones
  en curso laten en verde.
- **Pendientes**: cambios sin commit (`git status`) y marcas `TODO`, `FIXME` y
  `PENDIENTE` en los archivos tocados.
- Filtros por tipo y antigüedad, búsqueda, panel de detalle y «Abrir» en
  Finder o el explorador.
- **⚙ Ajustes**: tema oscuro / claro / sistema, tipo y tamaño de letra, forma
  del árbol, tamaño de las figuras, etiquetas, rejilla, niebla, giro
  automático, color de fondo y carpeta de Claude. Se recuerdan entre sesiones.
- **Clic derecho sobre un nodo**: «Ver solo esta rama» (proyecto, carpeta o
  sesión); «↩ Ver todo» o Esc para volver.

## Instalar

```sh
curl -fsSL https://raw.githubusercontent.com/airylp/grafoC/main/install.sh | sh
```

Descarga el código, lo compila (instala Rust con rustup si hace falta) y:

- **macOS**: crea `grafo.app` con ícono en `/Applications` (o `~/Applications`).
- **Linux**: instala `~/.local/bin/grafo` con ícono y entrada en el menú.

Si ya está instalado ofrece actualizar o desinstalar. Sin preguntas:

```sh
… | sh -s -- instalar        # instalar o actualizar
… | sh -s -- desinstalar     # pregunta si borrar también la configuración
```

Variables opcionales: `GRAFO_REPO` (usuario/repo), `GRAFO_REF` (rama o tag),
`GRAFO_DESTINO` (carpeta de instalación), `GRAFO_SOLO_BINARIO=1` (solo el
ejecutable en la carpeta actual), `GRAFO_IDIOMA=es|en|fr` (mensajes del
instalador).

En macOS también se puede usar `./empaquetar-mac.sh` desde un clon
(`crear`, `instalar`, `actualizar`, `desinstalar`, `--destino`, `--si`).

## Uso

- Arrastrar: girar (o mover el nodo bajo el ratón) · botón derecho: desplazar
  · rueda: zoom · doble clic: enfocar.
- La carpeta leída es `CLAUDE_CONFIG_DIR` o `~/.claude`; se puede cambiar en
  ⚙ Ajustes.
- Los ajustes se guardan en `~/Library/Application Support/grafo/ajustes.txt`
  (macOS) o `~/.config/grafo/ajustes.txt` (Linux).

## Compilar a mano

```sh
cargo run --release
cargo run --release -- --resumen     # cuenta lo que encuentra, sin ventana
```

`publicador/` es un pequeño servidor opcional (axum) para publicar y descargar
compilaciones en una red propia.

## Licencia

MIT
