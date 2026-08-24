#!/bin/sh
# Remove o adv-br com um comando:
#
#   curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/desinstalar.sh | sh
#
# Ou só algumas extensões:
#
#   ... | sh -s -- pjeoffice webpki
#
# Se o código estiver instalado (o caminho normal, se você instalou por curl),
# ele chama o ./desinstalar.sh de lá, que sabe desfazer a publicação e devolver
# as permissões dos navegadores. Se não estiver, faz o que dá sem ele: remove
# os Flatpaks e o que leva o nome do projeto na sua home.
set -eu

APP=io.github.llawli.AdvBr
CODIGO=${FLATPAK_ADV_BR_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/flatpak-adv-br}

vermelho='' verde='' azul='' normal=''
if [ -t 1 ]; then
    vermelho=$(printf '\033[1;31m') verde=$(printf '\033[1;32m')
    azul=$(printf '\033[1;34m') normal=$(printf '\033[0m')
fi
erro() { printf '%s ✗%s %s\n' "$vermelho" "$normal" "$*" >&2; exit 1; }
log()  { printf '%s::%s %s\n' "$azul" "$normal" "$*"; }
ok()   { printf '%s ✓%s %s\n' "$verde" "$normal" "$*"; }

command -v flatpak >/dev/null || erro "flatpak não encontrado."

if [ -x "$CODIGO/desinstalar.sh" ]; then
    "$CODIGO/desinstalar.sh" "$@"
    # O ./desinstalar.sh não apaga o próprio diretório, porque estaria puxando
    # o tapete de si mesmo. Aqui dá: este script veio pelo cano do curl.
    if [ $# -eq 0 ]; then
        rm -rf "$CODIGO"
        ok "código removido de $CODIGO"
    fi
    exit 0
fi

# Sem o código, a remoção seletiva não tem como saber o id de cada extensão a
# partir do nome curto. Só a completa faz sentido.
[ $# -eq 0 ] ||
    erro "para remover extensões pelo nome é preciso o código, que não está em
      $CODIGO. Instale-o de novo, ou remova pelo id:
          flatpak uninstall --user $APP.Assinador.WebPKI"

log "o código não está em $CODIGO; removendo o que dá sem ele"

# As extensões saem junto com o app (todas são declaradas com autodelete), mas
# pedir explicitamente é o que faz a mensagem dizer o que saiu.
for ref in $(flatpak list --columns=application 2>/dev/null | grep "^$APP" || true); do
    flatpak uninstall --user -y "$ref" >/dev/null 2>&1 && ok "$ref removido"
done

# O que a publicação escreve leva sempre o prefixo adv-br- ou o id do app.
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}"/pkcs11/modules/adv-br-*.module 2>/dev/null || true
rm -f "$HOME"/.local/bin/adv-br-* 2>/dev/null || true
rm -f "${XDG_DATA_HOME:-$HOME/.local/share}"/applications/"$APP".*.desktop 2>/dev/null || true
rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/$APP" 2>/dev/null || true
rm -rf "$HOME/.var/app/$APP" 2>/dev/null || true
rm -rf "${XDG_CACHE_HOME:-$HOME/.cache}/flatpak-adv-br" 2>/dev/null || true
find "$HOME/.var/app" -maxdepth 5 -path '*/data/adv-br*' -exec rm -rf {} + 2>/dev/null || true
ok "arquivos do projeto removidos da sua home"

printf '\n  Os manifestos dos assinadores nos navegadores e os registros nos bancos\n'
printf '  NSS precisam do código para serem desfeitos com precisão. Se você os quer\n'
printf '  fora, reinstale o código e rode o desinstalador completo:\n\n'
printf '      curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/install.sh | sh -s -- --sem-publicar\n'
printf '      %s/desinstalar.sh\n\n' "$CODIGO"
