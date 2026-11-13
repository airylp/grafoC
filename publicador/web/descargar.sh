#!/bin/sh
# Descarga la última grafo.app ya compilada del servidor de casa y la instala (sin compilar).
#
#   curl -fsSL {{SERVIDOR}}/descargar.sh | sh
set -eu

SERVIDOR="{{SERVIDOR}}"
falla() { echo "✗ $*" >&2; exit 1; }

[ "$(uname)" = "Darwin" ] || falla "Este script es para macOS."
case "$(uname -m)" in
    arm64) PLATAFORMA=macos-arm64 ;;
    x86_64) PLATAFORMA=macos-x86_64 ;;
    *) falla "Arquitectura no soportada: $(uname -m)" ;;
esac

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

echo "==> Descargando la última versión para $PLATAFORMA"
curl -fsSL -D "$TMP/cabeceras" -o "$TMP/grafo.zip" "$SERVIDOR/ultima/$PLATAFORMA" \
    || falla "No hay ninguna versión publicada para $PLATAFORMA. Publica una con: curl -fsSL $SERVIDOR/instalar.sh | GRAFO_TOKEN=… sh"

ESPERADO=$(tr -d '\r' < "$TMP/cabeceras" | sed -n 's/^[Xx]-[Ss][Hh][Aa]256: *//p' | tail -n 1)
OBTENIDO=$(shasum -a 256 "$TMP/grafo.zip" | cut -d ' ' -f 1)
if [ -n "$ESPERADO" ] && [ "$ESPERADO" != "$OBTENIDO" ]; then
    falla "La suma SHA-256 no coincide: la descarga está dañada."
fi

ditto -x -k "$TMP/grafo.zip" "$TMP/app"
[ -d "$TMP/app/grafo.app" ] || falla "El paquete descargado no contiene grafo.app"

if pgrep -f "grafo.app/Contents/MacOS/grafo" >/dev/null 2>&1; then
    echo "==> Cerrando grafo"
    osascript -e 'tell application id "io.github.airylp.grafo" to quit' >/dev/null 2>&1 || true
    sleep 1
fi

DESTINO=/Applications
[ -w "$DESTINO" ] || { DESTINO="$HOME/Applications"; mkdir -p "$DESTINO"; }
rm -rf "$DESTINO/grafo.app"
cp -R "$TMP/app/grafo.app" "$DESTINO/"
xattr -dr com.apple.quarantine "$DESTINO/grafo.app" 2>/dev/null || true

echo "✓ Instalada en $DESTINO/grafo.app"
open "$DESTINO/grafo.app" 2>/dev/null || true
echo "  Ábrela desde Launchpad, Spotlight («Grafo de Claude») o con: open \"$DESTINO/grafo.app\""
