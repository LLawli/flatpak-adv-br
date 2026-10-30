#!/usr/bin/env bash
# Ponta a ponta dos aplicativos: o Âncora acerta o Papers e o Firefox numa
# home falsa, e o NSS de dentro do Firefox Flatpak de verdade enxerga o token
# pelo módulo registrado, exatamente uma vez.
#
# A cadeia é a mesma de produção: NSS do Firefox -> p11-kit-client.so do
# runtime -> servidor do sistema -> servidor do Âncora -> SoftHSM. O
# "servidor do sistema" aqui é um `p11-kit server` do host lendo a home falsa,
# e não o p11-kit-server.socket da sessão, que serviria os tokens reais.
#
# `ancora configurar` liga o p11-kit-server.socket do usuário no host
# (`systemctl --user enable --now`), o que é inócuo se ele já está ligado.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

command -v p11-kit >/dev/null || { echo "precisa do p11-kit no host"; exit 1; }
flatpak info org.mozilla.firefox >/dev/null 2>&1 || { echo "precisa do org.mozilla.firefox instalado"; exit 1; }

FIREFOX=$FALSA/.var/app/org.mozilla.firefox/config/mozilla/firefox
PAPERS=$FALSA/.var/app/org.gnome.Papers
OVERRIDES=$FALSA/.local/share/flatpak/overrides
SISTEMA_DIR=/run/user/$(id -u)/ancora-e2e
SISTEMA=$SISTEMA_DIR/sistema
rm -rf "$FIREFOX" "$PAPERS" "$OVERRIDES" "$BANCADA/dados"
mkdir -p "$SISTEMA_DIR"

instalar SoftHSM
subir
confere "o SoftHSM está publicado" "$(publicados)" "ancora-softhsm.module"

HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config p11-kit server -f -n "$SISTEMA" pkcs11: >/dev/null 2>&1 &
PARAR+=($!)
sleep 1
export ANCORA_SERVIDOR_DO_SISTEMA=$SISTEMA

# Papers: permissão e banco vazio. Não está instalado aqui, então a medição
# dentro dele não roda e fica a receita padrão, marcada.
saida=$(ancora configurar org.gnome.Papers)
echo "$saida" | sed 's/^/     /'
confere "Papers ganha a permissão" "$(grep -c 'xdg-run/p11-kit/pkcs11' "$OVERRIDES/org.gnome.Papers")" "1"
confere "Papers ganha um banco vazio" "$([ -f "$PAPERS/config/mozilla/firefox/ancora.default/cert9.db" ] && echo sim)" "sim"
confere "sem o Papers instalado, fica não verificado" "$(echo "$saida" | grep -c 'não verificado')" "1"

# Firefox: um perfil (o banco vazio que o Âncora acabou de criar serve). A
# medição roda no sandbox do Firefox de verdade, com o token-sentinela.
mkdir -p "$FIREFOX"
cp -r "$PAPERS/config/mozilla/firefox/ancora.default" "$FIREFOX/teste.default"
saida=$(ancora configurar org.mozilla.firefox)
echo "$saida" | sed 's/^/     /'
confere "a verificação escolhe registrar o cliente" \
    "$(echo "$saida" | grep -c 'aparece uma vez, registrado com /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so')" "1"
confere "o perfil do Firefox ganha o módulo" "$(grep -c '^name=Âncora (p11-kit)$' "$FIREFOX/teste.default/pkcs11.txt")" "1"
confere "o sentinela sai ao terminar" "$(ls "$MODULES" | grep -c verificacao || true)" "0"
confere "e ninguém mais o vê" "$(tokens_no_host | grep -c 'verificação' || true)" "0"

vistos=$(flatpak run --filesystem="$BANCADA" --filesystem="$SISTEMA_DIR" \
    --env=LD_LIBRARY_PATH=/app/lib/firefox --command=python3 org.mozilla.firefox \
    -I "$OBRA/nss.py" "$FIREFOX/teste.default" "$SISTEMA")
echo "     o NSS do Firefox vê: $vistos"
confere "o Firefox vê o token uma vez, pelo módulo do Âncora" \
    "$(echo "$vistos" | python3 -I -c 'import json,sys; print(sum(1 for m,t in json.load(sys.stdin) if t == "HUBTESTE" and m == "Âncora (p11-kit)"))')" "1"
confere "e por nenhum outro módulo" \
    "$(echo "$vistos" | python3 -I -c 'import json,sys; print(sum(1 for m,t in json.load(sys.stdin) if t == "HUBTESTE"))')" "1"

# Desfazer tira só o que o Âncora pôs.
ancora desfazer org.mozilla.firefox
ancora desfazer org.gnome.Papers
confere "desfazer tira o módulo do perfil" "$(grep -c 'Âncora' "$FIREFOX/teste.default/pkcs11.txt" || true)" "0"
confere "desfazer apaga o override que o Âncora criou" "$(ls "$OVERRIDES" | tr '\n' ' ')" ""
confere "desfazer apaga o banco do Papers" "$([ -e "$PAPERS/config/mozilla/firefox/ancora.default" ] && echo ainda || echo saiu)" "saiu"

kill -TERM "$(pid_servico)"; sleep 3
rm -rf "$SISTEMA_DIR"
remover SoftHSM
terminar
