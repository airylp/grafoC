#!/usr/bin/env sh
# grafo: descargar, compilar, instalar y abrir con un solo comando.
#
#     curl -fsSL {{SERVIDOR}}/instalar.sh | sh
#
#   macOS: crea grafo.app en /Applications (o ~/Applications sin permiso) y la abre.
#   Linux: instala ~/.local/bin/grafo con entrada en el menú de aplicaciones y lo abre.
#
#   … | sh -s -- desinstalar          quita la app y la caché de compilación
#   … | GRAFO_TOKEN=xxxx sh           además publica la app compilada en el servidor
#                                     (el token está en el servidor: ~/.local/share/grafo-publicador/token)
#
# Variables opcionales: GRAFO_NO_ABRIR=1 (no abrir al terminar), GRAFO_NOTA="texto" (nota al publicar).
set -eu

SERVIDOR="{{SERVIDOR}}"
ACCION="${1:-instalar}"
OS="$(uname)"
BUNDLE_ID="io.github.airylp.grafo"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }

if [ "$OS" = "Darwin" ]; then
    CACHE="$HOME/Library/Caches/grafo-compilacion"
    APP_DIRS="/Applications $HOME/Applications"
else
    CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/grafo-compilacion"
    BIN_DIR="$HOME/.local/bin"
    DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
fi

# ------------------------------------------------------------ desinstalar
if [ "$ACCION" = "desinstalar" ]; then
    if [ "$OS" = "Darwin" ]; then
        osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || true
        for d in $APP_DIRS; do
            [ -d "$d/grafo.app" ] && rm -rf "${d:?}/grafo.app" && info "Quitada $d/grafo.app"
        done
    else
        for f in "$BIN_DIR/grafo" "$DATA/applications/grafo.desktop"; do
            [ -e "$f" ] && rm -f "$f" && info "Quitado $f"
        done
    fi
    rm -rf "$CACHE"
    info "grafo desinstalado"
    exit 0
fi
[ "$ACCION" = "instalar" ] || fail "acción desconocida: $ACCION (usa instalar o desinstalar)"

# ------------------------------------------------------------ requisitos
command -v curl >/dev/null 2>&1 || fail "se necesita curl"
command -v tar  >/dev/null 2>&1 || fail "se necesita tar"
command -v cc   >/dev/null 2>&1 || fail "se necesita un compilador/enlazador C (cc).
  macOS:         xcode-select --install
  Debian/Ubuntu: sudo apt install build-essential"

# Rust: usa el instalado o lo instala con rustup en ~/.cargo (sin sudo).
PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
    info "Rust no encontrado; instalando con rustup en ~/.cargo"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path
fi
export PATH
info "Usando $(cargo --version)"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

info "Descargando el código de $SERVIDOR"
mkdir -p "$TMP/fuente"
curl -fsSL "$SERVIDOR/fuente.tar.gz" | tar -xzf - -C "$TMP/fuente"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$TMP/fuente/Cargo.toml" | head -n 1)

# Caché de compilación: las actualizaciones siguientes tardan mucho menos.
CARGO_TARGET_DIR="$CACHE"
export CARGO_TARGET_DIR
info "Compilando grafo $VERSION (la primera vez tarda unos minutos)"
(cd "$TMP/fuente" && cargo build --release --locked)

# ------------------------------------------------------------ macOS
if [ "$OS" = "Darwin" ]; then
    case "$(uname -m)" in arm64) PLATAFORMA=macos-arm64 ;; *) PLATAFORMA="macos-$(uname -m)" ;; esac
    APP="$TMP/grafo.app"
    mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
    cp "$CACHE/release/grafo" "$APP/Contents/MacOS/grafo"
    cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>grafo</string>
    <key>CFBundleDisplayName</key><string>Grafo de Claude</string>
    <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key><string>grafo</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleDevelopmentRegion</key><string>es</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
    # Firma local (ad-hoc): suficiente para abrirla en tus Macs.
    codesign --force --deep --sign - "$APP" >/dev/null 2>&1 || info "Aviso: no se pudo firmar (funciona igual)"

    if pgrep -f "grafo.app/Contents/MacOS/grafo" >/dev/null 2>&1; then
        info "Cerrando grafo para reemplazarlo"
        osascript -e "tell application id \"$BUNDLE_ID\" to quit" >/dev/null 2>&1 || true
        sleep 1
    fi
    DESTINO=/Applications
    [ -w "$DESTINO" ] || { DESTINO="$HOME/Applications"; mkdir -p "$DESTINO"; }
    if [ -d "$DESTINO/grafo.app" ]; then
        # Reemplaza el contenido sin borrar la carpeta: el Dock la sigue reconociendo.
        rsync -a --delete "$APP/" "$DESTINO/grafo.app/"
    else
        cp -R "$APP" "$DESTINO/"
    fi
    /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$DESTINO/grafo.app" >/dev/null 2>&1 || true
    info "Instalada en $DESTINO/grafo.app"

    if [ -n "${GRAFO_TOKEN:-}" ]; then
        info "Publicando grafo $VERSION ($PLATAFORMA) en $SERVIDOR"
        ditto -c -k --keepParent "$APP" "$TMP/grafo-$PLATAFORMA.zip"
        curl -fsS -o /dev/null -X POST \
            -H "x-token: $GRAFO_TOKEN" \
            -H "Content-Type: application/octet-stream" \
            --data-binary @"$TMP/grafo-$PLATAFORMA.zip" \
            --url-query "plataforma=$PLATAFORMA" \
            --url-query "archivo=grafo-$PLATAFORMA.zip" \
            --url-query "version=$VERSION" \
            --url-query "nota=${GRAFO_NOTA:-Compilado en $(scutil --get ComputerName 2>/dev/null || hostname)}" \
            "$SERVIDOR/publicar" && info "Publicada" || info "Aviso: no se pudo publicar (¿token correcto?)"
    fi

    if [ "${GRAFO_NO_ABRIR:-0}" != 1 ]; then
        info "Abriendo grafo"
        open "$DESTINO/grafo.app"
    fi

# ------------------------------------------------------------ Linux
else
    mkdir -p "$BIN_DIR" "$DATA/applications"
    # Copia y renombra: el ejecutable en uso sigue funcionando hasta reiniciarlo.
    cp "$CACHE/release/grafo" "$BIN_DIR/.grafo.nuevo"
    chmod +x "$BIN_DIR/.grafo.nuevo"
    mv -f "$BIN_DIR/.grafo.nuevo" "$BIN_DIR/grafo"
    cat > "$DATA/applications/grafo.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Grafo de Claude
Comment=Carpetas, proyectos, sesiones, archivos y pendientes de Claude en 3D
Exec=$BIN_DIR/grafo
Icon=applications-science
Terminal=false
Categories=Utility;Development;
DESKTOP
    info "Instalado: $BIN_DIR/grafo (y en el menú de aplicaciones)"

    if [ -n "${GRAFO_TOKEN:-}" ]; then
        PLATAFORMA="linux-$(uname -m)"
        info "Publicando grafo $VERSION ($PLATAFORMA) en $SERVIDOR"
        tar -czf "$TMP/grafo-$PLATAFORMA.tar.gz" -C "$BIN_DIR" grafo
        curl -fsS -o /dev/null -X POST \
            -H "x-token: $GRAFO_TOKEN" \
            -H "Content-Type: application/octet-stream" \
            --data-binary @"$TMP/grafo-$PLATAFORMA.tar.gz" \
            --url-query "plataforma=$PLATAFORMA" \
            --url-query "archivo=grafo-$PLATAFORMA.tar.gz" \
            --url-query "version=$VERSION" \
            --url-query "nota=${GRAFO_NOTA:-Compilado en $(hostname)}" \
            "$SERVIDOR/publicar" && info "Publicada" || info "Aviso: no se pudo publicar (¿token correcto?)"
    fi

    if [ "${GRAFO_NO_ABRIR:-0}" != 1 ] && { [ -n "${DISPLAY:-}" ] || [ -n "${WAYLAND_DISPLAY:-}" ]; }; then
        info "Abriendo grafo"
        nohup "$BIN_DIR/grafo" >/dev/null 2>&1 &
    fi
fi

# La caché acelera las actualizaciones; si pasa de 3 GB se vacía.
if [ -d "$CACHE" ] && [ "$(du -sk "$CACHE" | cut -f1)" -gt 3145728 ]; then
    rm -rf "$CACHE" && info "Caché de compilación vaciada (pasaba de 3 GB)"
fi
echo "   Para actualizar más adelante, vuelve a ejecutar este mismo comando."
