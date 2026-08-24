#!/usr/bin/env bash
# Remove o adv-br, inteiro ou por partes.
#
#   ./desinstalar.sh                    tudo
#   ./desinstalar.sh pjeoffice          só o PJeOffice, e republica o resto
#   ./desinstalar.sh webpki certisign   mais de um de uma vez
#
# O que ele nunca apaga: a sua configuração. ~/.pjeoffice-pro e
# ~/.config/serproid são seus, foram feitos por você usando os programas, e
# continuam lá para quando você reinstalar.
set -euo pipefail

RAIZ=$(cd "$(dirname "$0")" && pwd)
cd "$RAIZ"

. "$RAIZ/host/comum.sh"
# Lido pelo host/extensoes.sh, que resolve os manifestos a partir daqui.
# shellcheck disable=SC2034
AQUI_RAIZ=$RAIZ
. "$RAIZ/host/extensoes.sh"

ALVOS=()
MANTER_PERMISSOES=0

ajuda() {
    /usr/bin/cat <<'FIM'
Uso: ./desinstalar.sh [opções] [extensões...]

Sem nenhuma extensão nomeada, remove tudo: as extensões, o pacote base, o que
foi publicado nos navegadores, as permissões concedidas a eles e os artefatos
de construção.

Com uma ou mais extensões, remove só elas e republica o que ficou:

  safesign  safenet  serproid        drivers de token
  webpki    websigner  certisign     assinadores em navegador
  pjeoffice                          PJeOffice Pro

  --manter-permissoes  não mexe nos 'flatpak override' dos navegadores
  --ajuda              esta mensagem

Nunca são tocados: ~/.pjeoffice-pro e ~/.config/serproid, que são seus.
FIM
}

while [ $# -gt 0 ]; do
    case $1 in
        --manter-permissoes) MANTER_PERMISSOES=1 ;;
        --ajuda|-h)          ajuda; exit 0 ;;
        -*)                  erro "opção desconhecida: $1 (use --ajuda)" ;;
        *)
            [ -n "${EXTENSOES[$1]:-}" ] ||
                erro "não conheço a extensão '$1'. Veja ./desinstalar.sh --ajuda"
            ALVOS+=("$1")
            ;;
    esac
    shift
done

command -v flatpak >/dev/null || erro "flatpak não encontrado no PATH."

# ---------------------------------------------------------------------------
# Remoção parcial: sai daqui.
# ---------------------------------------------------------------------------
if [ ${#ALVOS[@]} -gt 0 ]; then
    titulo "Removendo extensões"
    for alvo in "${ALVOS[@]}"; do
        extensao=$(id_da_extensao "$alvo") ||
            { aviso "não achei o manifesto de $alvo; pulando."; continue; }
        if flatpak info --user "$extensao" >/dev/null 2>&1; then
            flatpak uninstall --user -y "$extensao" >/dev/null && ok "$alvo removido"
        else
            log "$alvo não estava instalado."
        fi
    done

    # Republicar é o que apaga o .module do driver que saiu e o manifesto do
    # assinador que saiu. Sem isto, o p11-kit tentaria abrir um caminho que já
    # não existe a cada abertura de navegador.
    if flatpak_instalado "$APP_ID"; then
        titulo "Republicando o que ficou"
        "$RAIZ/host/publicar.sh"
    fi
    exit 0
fi

# ---------------------------------------------------------------------------
# Remoção completa.
# ---------------------------------------------------------------------------
titulo "1/4 · Desfazendo a publicação"
if flatpak_instalado "$APP_ID"; then
    "$RAIZ/host/publicar.sh" --remover
else
    log "o pacote não está instalado; limpando o que ficou na home."
    # O --remover precisa do pacote para saber o que publicou. Sem ele, o que
    # dá para fazer é apagar o que leva o nosso prefixo.
    rm -f "$MODULOS_HOST/$PREFIXO_MODULO"*.module 2>/dev/null || true
    rm -f "$BIN_HOST/$PREFIXO_WRAPPER"* 2>/dev/null || true
    rm -f "$ATALHOS_HOST/$APP_ID."*.desktop 2>/dev/null || true
    rm -rf "$ICONES_HOST" 2>/dev/null || true
fi

titulo "2/4 · Devolvendo as permissões dos navegadores"
if [ "$MANTER_PERMISSOES" = 1 ]; then
    log "mantidas, a pedido."
else
    # Uma chave de cada vez, e não 'flatpak override --reset': o --reset
    # zeraria também o que outro programa concedeu ao mesmo navegador, como os
    # caminhos que o KeePassXC pede. Ver host/overrides.py.
    for permissao in "${PERMISSOES_CONCEDIDAS[@]}"; do
        # shellcheck disable=SC2086
        set -- $permissao
        resultado=$(python3 "$RAIZ/host/overrides.py" remover "$1" "$2" "$3" 2>/dev/null || true)
        case "$resultado" in
            "chave removida"|"arquivo removido") ok "$1: $3" ;;
        esac
    done

    if systemctl --user is-enabled p11-kit-server.socket >/dev/null 2>&1; then
        systemctl --user disable --now p11-kit-server.socket >/dev/null 2>&1 &&
            ok "p11-kit-server.socket desligado"
        printf '   Se outro projeto usa esse socket (o sora, por exemplo), religue:\n'
        printf '       systemctl --user enable --now p11-kit-server.socket\n'
    fi
fi

titulo "3/4 · Removendo os Flatpaks"
for alvo in "${!EXTENSOES[@]}"; do
    extensao=$(id_da_extensao "$alvo") || continue
    flatpak info --user "$extensao" >/dev/null 2>&1 || continue
    flatpak uninstall --user -y "$extensao" >/dev/null && ok "$alvo"
done
if flatpak_instalado "$APP_ID"; then
    flatpak uninstall --user -y "$APP_ID" >/dev/null && ok "$APP_ID"
fi
flatpak uninstall --user -y "$APP_ID.Debug" >/dev/null 2>&1 && ok "$APP_ID.Debug" || true
rm -rf "$HOME/.var/app/$APP_ID" 2>/dev/null || true

titulo "4/4 · Artefatos de construção"
TRABALHO=${ADV_BR_TRABALHO:-${XDG_CACHE_HOME:-$HOME/.cache}/flatpak-adv-br/construcao}
if [ -d "$TRABALHO" ]; then
    tamanho=$(du -sh "$TRABALHO" 2>/dev/null | cut -f1)
    rm -rf "$TRABALHO" && ok "$TRABALHO ($tamanho)"
else
    log "nada em $TRABALHO."
fi

CODIGO=${XDG_DATA_HOME:-$HOME/.local/share}/flatpak-adv-br
/usr/bin/cat <<FIM

  Pronto. O que continua no seu sistema, de propósito:

      ~/.pjeoffice-pro        sua configuração do PJeOffice
      ~/.config/serproid      seus certificados em nuvem do Serpro

FIM

# Este script pode estar DENTRO do diretório que sobrou: apagá-lo enquanto roda
# é pedir para o bash ler um arquivo que já não existe. Fica como último passo,
# para quem quiser.
if [ -d "$CODIGO" ]; then
    printf '  E o código, se você não for reinstalar:\n\n      rm -rf %s\n\n' "$CODIGO"
fi
