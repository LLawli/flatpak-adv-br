#!/bin/sh
# Instala o adv-br com um comando, sem clonar nada à mão:
#
#   curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/install.sh | sh
#
# Com opções, que são as mesmas do ./instalar.sh:
#
#   ... | sh -s -- --with-safesign --with-webpki
#
# Não existe um .flatpak pronto para baixar porque nada aqui pode ser
# redistribuído: os drivers e os assinadores são dos fabricantes, e cada um é
# baixado da URL deles, na sua máquina, com sha256 conferido. Por isso o
# instalador traz o código e constrói.
#
# O que fica no seu sistema depois disto:
#
#   ~/.local/share/flatpak-adv-br   o código, que você usa depois para publicar
#                                   e diagnosticar (algumas centenas de KB)
#   os Flatpaks instalados          o pacote e as extensões que você pediu
#
# O que NÃO fica: nenhum artefato de construção. Eles são feitos em
# ~/.cache/flatpak-adv-br/construcao e apagados no fim — o .flatpak-builder de uma
# instalação completa passa de 1 GB, e ninguém que instalou por curl deveria
# ter que descobrir isso depois.
set -eu

REPO=${FLATPAK_ADV_BR_REPO:-https://github.com/LLawli/flatpak-adv-br}
RAMO=${FLATPAK_ADV_BR_RAMO:-main}
DESTINO=${FLATPAK_ADV_BR_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/flatpak-adv-br}

vermelho='' verde='' azul='' normal=''
if [ -t 1 ]; then
    vermelho=$(printf '\033[1;31m') verde=$(printf '\033[1;32m')
    azul=$(printf '\033[1;34m') normal=$(printf '\033[0m')
fi
erro() { printf '%s ✗%s %s\n' "$vermelho" "$normal" "$*" >&2; exit 1; }
log()  { printf '%s::%s %s\n' "$azul" "$normal" "$*"; }
ok()   { printf '%s ✓%s %s\n' "$verde" "$normal" "$*"; }

# Duas formas de trazer o código, e nenhuma delas exige que você tenha clonado
# antes. Com git, dá para atualizar depois sem baixar tudo de novo; sem git,
# o tarball resolve — e há máquina de trabalho onde instalar git é justamente o
# que a pessoa não pode fazer.
baixar_com_git() {
    if [ -d "$DESTINO/.git" ]; then
        log "atualizando $DESTINO"
        git -C "$DESTINO" fetch --quiet --depth 1 origin "$RAMO"
        git -C "$DESTINO" checkout --quiet "$RAMO" 2>/dev/null || true
        git -C "$DESTINO" reset --quiet --hard "origin/$RAMO"
    else
        log "baixando o código em $DESTINO"
        mkdir -p "$(dirname "$DESTINO")"
        git clone --quiet --depth 1 --branch "$RAMO" "$REPO" "$DESTINO"
    fi
}

baixar_com_tarball() {
    tmp=$(mktemp -d "${TMPDIR:-/tmp}/adv-br-XXXXXX")
    # O tarball é pequeno (código, sem binário nenhum), então /tmp serve.
    trap 'rm -rf "$tmp"' EXIT INT TERM

    log "baixando o código (sem git) em $DESTINO"
    curl -fsSL "$REPO/archive/refs/heads/$RAMO.tar.gz" -o "$tmp/fonte.tar.gz" ||
        erro "não consegui baixar $REPO/archive/refs/heads/$RAMO.tar.gz"

    rm -rf "$DESTINO"
    mkdir -p "$DESTINO"
    # O tarball do GitHub tem um diretório <repo>-<ramo> no topo.
    tar -xzf "$tmp/fonte.tar.gz" -C "$DESTINO" --strip-components=1 ||
        erro "não consegui extrair o código."
}

command -v curl >/dev/null || erro "curl não encontrado."

if command -v git >/dev/null; then
    baixar_com_git
else
    baixar_com_tarball
fi

[ -x "$DESTINO/instalar.sh" ] ||
    erro "o código baixado não tem instalar.sh; algo saiu errado em $DESTINO."

ok "código em $DESTINO"

# --limpar: os artefatos de construção somem no fim. Quem clonou à mão e quer
# guardá-los para a próxima construção roda o ./instalar.sh direto, sem isto.
exec "$DESTINO/instalar.sh" --limpar "$@"
