#!/bin/sh
# Instalador de uma linha:
#
#   curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/install.sh | sh
#
# Clona (ou atualiza) em ~/.local/share/flatpak-adv-br e roda o ./instalar.sh,
# repassando as opções:
#
#   ... | sh -s -- --with-safesign
set -eu

REPO=${FLATPAK_ADV_BR_REPO:-https://github.com/LLawli/flatpak-adv-br}
DESTINO=${FLATPAK_ADV_BR_DIR:-$HOME/.local/share/flatpak-adv-br}

erro() { printf '\033[1;31m ✗\033[0m %s\n' "$*" >&2; exit 1; }
log()  { printf '\033[1;34m::\033[0m %s\n' "$*"; }

command -v git >/dev/null || erro "git não encontrado; instale-o e rode de novo."

if [ -d "$DESTINO/.git" ]; then
    log "atualizando $DESTINO ..."
    git -C "$DESTINO" pull --ff-only
else
    log "clonando em $DESTINO ..."
    mkdir -p "$(dirname "$DESTINO")"
    git clone --depth 1 "$REPO" "$DESTINO"
fi

exec "$DESTINO/instalar.sh" "$@"
