#!/usr/bin/env sh
# grafo: grafo 3D de las carpetas, proyectos, sesiones, archivos y pendientes de Claude.
# Instalar, actualizar o desinstalar con un solo comando.
#
#     curl -fsSL https://raw.githubusercontent.com/airylp/grafoC/main/install.sh | sh
#
#   Muestra un menú: si ya está instalado indica la versión instalada y la última,
#   y ofrece actualizar o desinstalar; si no, instalarlo. También se puede indicar
#   directamente (sin menú):
#     … | sh -s -- instalar        (instalar o actualizar)
#     … | sh -s -- actualizar      (lo mismo)
#     … | sh -s -- desinstalar     (pregunta si borrar también la configuración)
#     … | sh -s -- comprobar       (solo muestra la versión instalada y la última)
#
#   macOS: crea grafo.app (con ícono) en /Applications (o ~/Applications sin permiso).
#   Linux: instala ~/.local/bin/grafo con ícono y entrada en el menú de aplicaciones.
#
# Variables opcionales: GRAFO_REPO (usuario/repo), GRAFO_REF (rama o tag),
#   GRAFO_DESTINO (carpeta de instalación), GRAFO_SOLO_BINARIO=1 (solo el
#   ejecutable en la carpeta actual, sin instalar nada).
set -eu

REPO="${GRAFO_REPO:-airylp/grafoC}"
REF="${GRAFO_REF:-main}"
ACCION="${1:-}"
OS="$(uname)"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31merror:\033[0m %s\n' "$*" >&2; exit 1; }
# Mensajes en el idioma elegido: m "español" "english" "français".
m() { case "${IDIOMA:-es}" in en) printf '%s' "$2" ;; fr) printf '%s' "$3" ;; *) printf '%s' "$1" ;; esac; }

# Preguntas: se leen de la terminal (el script llega por la tubería de curl).
pregunta() {
    # ¿Hay terminal de verdad? (abrir /dev/tty falla si se ejecuta sin ella).
    if (: < /dev/tty) 2>/dev/null; then
        printf '%s %s ' "$1" "$(m '[s/N]' '[y/N]' '[o/N]')" > /dev/tty
        read -r r < /dev/tty || r=""
        case "$r" in s|S|si|sí|Sí|y|Y|yes|o|O|oui) return 0 ;; esac
    fi
    return 1
}

if [ "$OS" = "Darwin" ]; then
    # Con GRAFO_DESTINO solo se toca esa carpeta.
    APP_DIRS="${GRAFO_DESTINO:-/Applications $HOME/Applications}"
    CONFIG="$HOME/Library/Application Support/grafo"
    CACHE="$HOME/Library/Caches/grafo-compilacion"
else
    BIN_DIR="${GRAFO_DESTINO:-$HOME/.local/bin}"
    DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
    CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/grafo"
    CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/grafo-compilacion"
fi

hay_terminal() { (: < /dev/tty) 2>/dev/null; }
leer() { # leer "pregunta" → respuesta en $r
    printf '%s' "$1" > /dev/tty
    read -r r < /dev/tty || r=""
}

# ------------------------------------------------------------ idioma
# GRAFO_IDIOMA=es|en|fr, o el ya guardado en los ajustes de la app; si no
# hay ninguno y hay terminal, se pregunta (y se guarda para la app).
IDIOMA="${GRAFO_IDIOMA:-}"
GUARDAR_IDIOMA="$IDIOMA"
[ -n "$IDIOMA" ] || IDIOMA="$(sed -n 's/^idioma=//p' "$CONFIG/ajustes.txt" 2>/dev/null | head -n 1)"
if [ -z "$IDIOMA" ]; then
    case "${LC_ALL:-${LANG:-}}" in en*) IDIOMA=en ;; fr*) IDIOMA=fr ;; *) IDIOMA=es ;; esac
    if [ -z "$ACCION" ] && hay_terminal; then
        printf 'Idioma / Language / Langue\n  1) Español\n  2) English\n  3) Français\n' > /dev/tty
        case "$IDIOMA" in en) def=2 ;; fr) def=3 ;; *) def=1 ;; esac
        leer "[1-3] (Enter = $def): "
        case "${r:-$def}" in 2) IDIOMA=en ;; 3) IDIOMA=fr ;; *) IDIOMA=es ;; esac
        GUARDAR_IDIOMA="$IDIOMA"
    fi
fi
case "$IDIOMA" in es|en|fr) ;; *) IDIOMA=es ;; esac
export IDIOMA
# Deja el idioma en los ajustes de la app (sin tocar lo demás).
guardar_idioma() {
    [ -n "$GUARDAR_IDIOMA" ] || return 0
    mkdir -p "$CONFIG"
    f="$CONFIG/ajustes.txt"
    if [ -f "$f" ]; then grep -v '^idioma=' "$f" > "$f.tmp" || true; mv "$f.tmp" "$f"; fi
    echo "idioma=$IDIOMA" >> "$f"
}

# ------------------------------------------------------------ versiones
# Versión instalada («0.1.0 (abc1234)» o vacía) y último commit publicado.
version_instalada() {
    if [ "$OS" = "Darwin" ]; then
        for d in $APP_DIRS; do
            if [ -d "$d/grafo.app" ]; then
                defaults read "$d/grafo.app/Contents/Info" CFBundleVersion 2>/dev/null || echo "?"
                return 0
            fi
        done
    elif [ -x "$BIN_DIR/grafo" ]; then
        # La guarda la instalación (ejecutar el programa abriría la ventana en versiones viejas).
        cat "$DATA/grafo/version" 2>/dev/null || echo "?"
        return 0
    fi
    return 1
}
ultimo_commit() {
    curl -fsSL -H 'Accept: application/vnd.github.sha' "https://api.github.com/repos/$REPO/commits/$REF" 2>/dev/null | cut -c1-7 || true
}
INSTALADA=""
VERSION_ACTUAL="$(version_instalada || true)"
[ -n "$VERSION_ACTUAL" ] && INSTALADA="$(if [ "$OS" = "Darwin" ]; then for d in $APP_DIRS; do [ -d "$d/grafo.app" ] && { echo "$d/grafo.app"; break; }; done; else echo "$BIN_DIR/grafo"; fi)"

# ------------------------------------------------------------ comprobar
if [ "$ACCION" = "comprobar" ] || [ "$ACCION" = "version" ]; then
    ULTIMO="$(ultimo_commit)"
    if [ -n "$INSTALADA" ]; then
        echo "$(m 'Instalada' 'Installed' 'Installée'): $INSTALADA ($VERSION_ACTUAL)"
    else
        echo "$(m 'grafo no está instalado.' 'grafo is not installed.' 'grafo n’est pas installé.')"
    fi
    echo "$(m 'Última en GitHub' 'Latest on GitHub' 'Dernière sur GitHub'): ${ULTIMO:-?} ($REPO@$REF)"
    case "$VERSION_ACTUAL" in
        *"($ULTIMO)"*) [ -n "$ULTIMO" ] && echo "$(m 'Estás al día.' 'You are up to date.' 'Vous êtes à jour.')" ;;
        *) [ -n "$INSTALADA" ] && [ -n "$ULTIMO" ] && echo "$(m 'Hay una versión nueva: … | sh -s -- actualizar' 'A new version is available: … | sh -s -- actualizar' 'Une nouvelle version est disponible : … | sh -s -- actualizar')" ;;
    esac
    exit 0
fi

# ------------------------------------------------------------ ¿qué hacer?
# Sin acción indicada y con terminal, se pregunta. (La actualización desde la
# app indica «instalar» y no pregunta nada.)
if [ -z "$ACCION" ]; then
    ACCION=instalar
    if hay_terminal; then
        ULTIMO="$(ultimo_commit)"
        if [ -n "$INSTALADA" ]; then
            case "$VERSION_ACTUAL" in
                *"($ULTIMO)"*) al_dia=1 ;;
                *) al_dia=0 ;;
            esac
            [ -n "$ULTIMO" ] || al_dia=0
            {
                echo "$(m "grafo ya está instalado: $INSTALADA" "grafo is already installed: $INSTALADA" "grafo est déjà installé : $INSTALADA")"
                echo "   $(m 'Versión instalada' 'Installed version' 'Version installée'): $VERSION_ACTUAL"
                echo "   $(m 'Última en GitHub' 'Latest on GitHub' 'Dernière sur GitHub'):  ${ULTIMO:-?}"
                if [ "$al_dia" = 1 ]; then
                    echo "  1) $(m 'Reinstalar (ya tienes la última versión)' 'Reinstall (you already have the latest version)' 'Réinstaller (vous avez déjà la dernière version)')"
                else
                    echo "  1) $(m 'Actualizar a la última versión' 'Update to the latest version' 'Mettre à jour vers la dernière version')"
                fi
                echo "  2) $(m 'Desinstalar (pregunta si borrar también los ajustes)' 'Uninstall (asks whether to delete the settings too)' 'Désinstaller (demande s’il faut supprimer les réglages)')"
                echo "  3) $(m 'Cancelar' 'Cancel' 'Annuler')"
            } > /dev/tty
            leer "$(m 'Elige [1-3] (Enter = 1): ' 'Choose [1-3] (Enter = 1): ' 'Choisissez [1-3] (Entrée = 1) : ')"
            case "${r:-1}" in
                1) ACCION=instalar ;;
                2) ACCION=desinstalar ;;
                *) m "Cancelado." "Cancelled." "Annulé."; echo; exit 0 ;;
            esac
        else
            destino="/Applications"
            [ "$OS" = "Darwin" ] || destino="$BIN_DIR"
            {
                echo "$(m "grafo no está instalado. Última versión: ${ULTIMO:-?}" "grafo is not installed. Latest version: ${ULTIMO:-?}" "grafo n’est pas installé. Dernière version : ${ULTIMO:-?}")"
                echo "  1) $(m "Instalar en ${GRAFO_DESTINO:-$destino}" "Install in ${GRAFO_DESTINO:-$destino}" "Installer dans ${GRAFO_DESTINO:-$destino}")"
                echo "  2) $(m 'Cancelar' 'Cancel' 'Annuler')"
            } > /dev/tty
            leer "$(m 'Elige [1-2] (Enter = 1): ' 'Choose [1-2] (Enter = 1): ' 'Choisissez [1-2] (Entrée = 1) : ')"
            case "${r:-1}" in
                1) ACCION=instalar ;;
                *) m "Cancelado." "Cancelled." "Annulé."; echo; exit 0 ;;
            esac
        fi
    fi
fi

# ------------------------------------------------------------ desinstalar
if [ "$ACCION" = "desinstalar" ] || [ "$ACCION" = "--desinstalar" ]; then
    if [ "$OS" = "Darwin" ]; then
        for d in $APP_DIRS; do
            if [ -d "$d/grafo.app" ]; then
                [ -n "${GRAFO_DESTINO:-}" ] || osascript -e 'tell application id "io.github.airylp.grafo" to quit' >/dev/null 2>&1 || true
                /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$d/grafo.app" >/dev/null 2>&1 || true
                rm -rf "${d:?}/grafo.app" && info "$(m "Quitada $d/grafo.app" "Removed $d/grafo.app" "Supprimée : $d/grafo.app")"
            fi
        done
    else
        rm -rf "$DATA/grafo"
        for f in "$BIN_DIR/grafo" "$DATA/applications/grafo.desktop" "$DATA/icons/hicolor/256x256/apps/grafo.png"; do
            if [ -e "$f" ]; then rm -f "$f" && info "$(m "Quitado $f" "Removed $f" "Supprimé : $f")"; fi
        done
        command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" 2>/dev/null || true
    fi
    rm -rf "$CACHE"
    if [ -d "$CONFIG" ]; then
        echo "$(m 'Configuración' 'Settings' 'Réglages'): $CONFIG"
        ls -1 "$CONFIG" | sed 's/^/    /'
        if pregunta "$(m '¿Borrar también la configuración (tema, letra, vista…)?' 'Also delete the settings (theme, font, view…)?' 'Supprimer aussi les réglages (thème, police, vue…) ?')"; then
            rm -rf "$CONFIG" && info "$(m 'Configuración borrada' 'Settings deleted' 'Réglages supprimés')"
        else
            m "Configuración conservada." "Settings kept." "Réglages conservés."; echo
        fi
    fi
    info "$(m 'grafo desinstalado' 'grafo uninstalled' 'grafo désinstallé')"
    exit 0
fi
[ "$ACCION" = "instalar" ] || [ "$ACCION" = "actualizar" ] || fail "$(m "acción desconocida: $ACCION (usa instalar, actualizar, desinstalar o comprobar)" "unknown action: $ACCION (use instalar, actualizar, desinstalar or comprobar)" "action inconnue : $ACCION (utilisez instalar, actualizar, desinstalar ou comprobar)")"

# ------------------------------------------------------------ requisitos
command -v curl >/dev/null 2>&1 || fail "$(m 'se necesita curl' 'curl is required' 'curl est nécessaire')"
command -v tar  >/dev/null 2>&1 || fail "$(m 'se necesita tar' 'tar is required' 'tar est nécessaire')"
command -v cc   >/dev/null 2>&1 || fail "$(m 'se necesita un compilador/enlazador C (cc).' 'a C compiler/linker (cc) is required.' 'un compilateur/éditeur de liens C (cc) est nécessaire.')
  Debian/Ubuntu: sudo apt install build-essential
  Fedora:        sudo dnf install gcc
  Arch:          sudo pacman -S base-devel
  macOS:         xcode-select --install"

# Rust: usa el instalado o lo instala con rustup en ~/.cargo (sin sudo).
PATH="$HOME/.cargo/bin:$PATH"
if ! command -v cargo >/dev/null 2>&1; then
    info "$(m 'Rust no encontrado; instalando con rustup en ~/.cargo' 'Rust not found; installing it with rustup in ~/.cargo' 'Rust introuvable ; installation avec rustup dans ~/.cargo')"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --no-modify-path
fi
export PATH
info "$(m 'Usando' 'Using' 'Utilisation de') $(cargo --version)"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

info "$(m 'Descargando' 'Downloading' 'Téléchargement de') $REPO@$REF"
curl -fsSL "https://github.com/$REPO/archive/$REF.tar.gz" | tar -xz -C "$TMP" --strip-components=1
# Versión exacta (commit): se muestra en ⚙ Ajustes.
GRAFO_COMMIT="$(curl -fsSL -H 'Accept: application/vnd.github.sha' "https://api.github.com/repos/$REPO/commits/$REF" 2>/dev/null | cut -c1-7 || true)"
export GRAFO_COMMIT
# Caché de compilación: las actualizaciones siguientes tardan mucho menos.
CARGO_TARGET_DIR="$CACHE"
export CARGO_TARGET_DIR

if [ "${GRAFO_SOLO_BINARIO:-0}" = 1 ]; then
    info "$(m 'Compilando (la primera vez tarda unos minutos)' 'Building (the first time takes a few minutes)' 'Compilation (la première fois prend quelques minutes)')"
    (cd "$TMP" && cargo build --release --locked)
    cp "$CACHE/release/grafo" "./grafo"
    chmod +x ./grafo
    info "$(m 'Listo' 'Done' 'Terminé'): $(pwd)/grafo"
    exit 0
fi

if [ "$OS" = "Darwin" ]; then
    info "$(m 'Compilando y creando grafo.app (la primera vez tarda unos minutos)' 'Building grafo.app (the first time takes a few minutes)' 'Compilation de grafo.app (la première fois prend quelques minutes)')"
    # Si grafo está abierto, pregunta (en la terminal) si cerrarlo y reabrirlo;
    # con GRAFO_NO_CERRAR=1 no se cierra.
    sh "$TMP/empaquetar-mac.sh" instalar --si ${GRAFO_NO_CERRAR:+--no-cerrar} ${GRAFO_DESTINO:+--destino "$GRAFO_DESTINO"}
    guardar_idioma
    info "$(m 'Listo. Abre grafo desde Launchpad o Spotlight (Cmd+Espacio, «grafo»).' 'Done. Open grafo from Launchpad or Spotlight (Cmd+Space, “grafo”).' 'Terminé. Ouvrez grafo depuis Launchpad ou Spotlight (Cmd+Espace, « grafo »).')"
else
    info "$(m 'Compilando (la primera vez tarda unos minutos)' 'Building (the first time takes a few minutes)' 'Compilation (la première fois prend quelques minutes)')"
    (cd "$TMP" && cargo build --release --locked)
    mkdir -p "$BIN_DIR" "$DATA/applications" "$DATA/icons/hicolor/256x256/apps"
    # Copia y renombra: el ejecutable en uso sigue funcionando hasta reiniciarlo.
    cp "$CACHE/release/grafo" "$BIN_DIR/.grafo.nuevo"
    chmod +x "$BIN_DIR/.grafo.nuevo"
    mv -f "$BIN_DIR/.grafo.nuevo" "$BIN_DIR/grafo"
    "$BIN_DIR/grafo" --iconset "$TMP/iconos" && cp "$TMP/iconos/icon_256x256.png" "$DATA/icons/hicolor/256x256/apps/grafo.png"
    cat > "$DATA/applications/grafo.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=grafo
Comment=Grafo 3D de las carpetas, proyectos y sesiones de Claude
Comment[en]=3D graph of Claude folders, projects and sessions
Comment[fr]=Graphe 3D des dossiers, projets et sessions de Claude
Exec=$BIN_DIR/grafo
Icon=grafo
Terminal=false
Categories=Utility;Development;
StartupWMClass=grafo
DESKTOP
    command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" 2>/dev/null || true
    mkdir -p "$DATA/grafo"
    "$BIN_DIR/grafo" --version | sed 's/^grafo //' > "$DATA/grafo/version" || true
    guardar_idioma
    info "$(m "Instalado: $BIN_DIR/grafo (y en el menú de aplicaciones)" "Installed: $BIN_DIR/grafo (and in the applications menu)" "Installé : $BIN_DIR/grafo (et dans le menu des applications)")"
    case ":$PATH:" in *":$BIN_DIR:"*) ;; *) echo "   $(m "Nota: $BIN_DIR no está en tu PATH; añádelo para escribir «grafo» en la terminal." "Note: $BIN_DIR is not in your PATH; add it to run “grafo” from the terminal." "Remarque : $BIN_DIR n’est pas dans votre PATH ; ajoutez-le pour lancer « grafo » depuis le terminal.")" ;; esac
fi
# La caché de compilación acelera las actualizaciones, pero cargo no borra lo
# viejo: si pasa de 3 GB se vacía (la próxima compilación empieza de cero).
if [ -d "$CACHE" ] && [ "$(du -sk "$CACHE" | cut -f1)" -gt 3145728 ]; then
    rm -rf "$CACHE" && info "$(m 'Caché de compilación vaciada (pasaba de 3 GB)' 'Build cache emptied (it was over 3 GB)' 'Cache de compilation vidé (il dépassait 3 Go)')"
fi
echo "   $(m 'Para actualizar más adelante vuelve a ejecutar este comando (también en ⚙ Ajustes → «Copiar comando»).' 'To update later, run this command again (also in ⚙ Settings → “Copiar comando”).' 'Pour mettre à jour plus tard, relancez cette commande (aussi dans ⚙ Réglages → « Copiar comando »).')"
