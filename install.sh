#!/usr/bin/env bash
set -euo pipefail

# Corn Mato installer
#
# Installs this source tree's mato binary into ~/.local/bin by default.
# It intentionally does not use upstream release assets because this fork's
# behavior differs from upstream Mato.
#
# Usage from the repository root:
#   ./install.sh
#
# Useful options:
#   INSTALL_DIR="$HOME/.local/bin" ./install.sh
#   RESET_STATE=1 ./install.sh        # backup old ~/.config/mato/state.json
#   KEEP_DAEMON=1 ./install.sh        # do not stop a running mato daemon

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
TARGET_BIN="$INSTALL_DIR/mato"
RESET_STATE="${RESET_STATE:-0}"
KEEP_DAEMON="${KEEP_DAEMON:-0}"

log() {
  printf '%s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

need_cmd cargo
need_cmd install

if [ ! -f "$ROOT_DIR/Cargo.toml" ]; then
  die "install.sh must live in the Mato repository root"
fi

log "Installing Corn Mato from: $ROOT_DIR"
log "Target binary: $TARGET_BIN"

ACTIVE_MATO="$(command -v mato 2>/dev/null || true)"
if [ -n "$ACTIVE_MATO" ]; then
  log "Current mato in PATH: $ACTIVE_MATO"
  log "Current version: $("$ACTIVE_MATO" --version 2>/dev/null || true)"
fi

if [ "$KEEP_DAEMON" != "1" ]; then
  if pgrep -u "$USER" -f 'mato --daemon' >/dev/null 2>&1; then
    log "Stopping running Mato daemon before replacing the binary..."
    if [ -x "$TARGET_BIN" ]; then
      "$TARGET_BIN" --kill || true
    elif [ -n "$ACTIVE_MATO" ]; then
      "$ACTIVE_MATO" --kill || true
    else
      pkill -u "$USER" -f 'mato --daemon' || true
    fi
  fi
else
  log "KEEP_DAEMON=1 set; not stopping running daemon."
fi

BACKUP_SUFFIX="$(date +%Y%m%d-%H%M%S)"
if [ -f "$TARGET_BIN" ]; then
  BACKUP_BIN="${TARGET_BIN}.backup-${BACKUP_SUFFIX}"
  log "Backing up existing target binary to: $BACKUP_BIN"
  cp "$TARGET_BIN" "$BACKUP_BIN"
fi

if [ "$RESET_STATE" = "1" ] && [ -f "$HOME/.config/mato/state.json" ]; then
  STATE_BACKUP="$HOME/.config/mato/state.json.backup-${BACKUP_SUFFIX}"
  log "Backing up old state to: $STATE_BACKUP"
  mv "$HOME/.config/mato/state.json" "$STATE_BACKUP"
fi

log "Building release binary..."
cargo install --path "$ROOT_DIR" --force

mkdir -p "$INSTALL_DIR"
install -m 755 "$HOME/.cargo/bin/mato" "$TARGET_BIN"

mkdir -p "$HOME/.config/mato" "$HOME/mato_corn"
if [ ! -f "$HOME/.config/mato/theme.toml" ]; then
  printf 'name = "corn"\n' > "$HOME/.config/mato/theme.toml"
fi

log ""
log "Installed: $("$TARGET_BIN" --version)"
log "Binary:    $TARGET_BIN"
log "Desk root: $HOME/mato_corn"
log "Theme:     $HOME/.config/mato/theme.toml"
log ""

case ":$PATH:" in
  *":$INSTALL_DIR:"*)
    log "PATH already contains $INSTALL_DIR"
    ;;
  *)
    log "PATH does not contain $INSTALL_DIR."
    log "Add this to your shell rc file:"
    log "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac

RESOLVED="$(command -v mato 2>/dev/null || true)"
if [ "$RESOLVED" != "$TARGET_BIN" ]; then
  log ""
  log "PATH priority warning:"
  log "  mato currently resolves to: ${RESOLVED:-<not found>}"
  log "  installed binary is:        $TARGET_BIN"
  log "Run directly with: $TARGET_BIN"
else
  log "Ready: run 'mato'"
fi
