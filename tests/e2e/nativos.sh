#!/usr/bin/env bash
# Ponta a ponta dos navegadores nativos: o Âncora publica e despublica nos
# bancos NSS do Firefox nativo e do ~/.pki/nssdb (Chromium e derivados) de uma
# home falsa. O NSS do host precisa ver o token uma vez, pelo módulo do Âncora;
# o mesmo ~/.pki/nssdb aberto de dentro de um sandbox (como o Chromium Flatpak
# o abre) não pode ver o token, para não duplicar.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

FIREFOX=$FALSA/.mozilla/firefox/teste.default
SISTEMA=$FALSA/.pki/nssdb
rm -rf "$FALSA/.mozilla" "$FALSA/.pki" "$FALSA/.var/app/org.chromium.Chromium"

banco_vazio() {
    mkdir -p "$1"
    python3 -I -c 'import ctypes,sys; n=ctypes.CDLL("libnss3.so"); assert n.NSS_Initialize(("sql:"+sys.argv[1]).encode(),b"",b"",b"secmod.db",0)==0; n.NSS_Shutdown()' "$1"
}
# Quantas vezes o NSS do host vê o token de teste, e por qual módulo.
no_host() {
    HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config python3 -I "$AQUI/nss.py" "$1" |
        python3 -I -c 'import json,sys; print(" ".join(m for m,t in json.load(sys.stdin) if t == "HUBTESTE"))'
}

for c in /usr/lib64/p11-kit-proxy.so "/usr/lib/$(uname -m)-linux-gnu/p11-kit-proxy.so" /usr/lib/p11-kit-proxy.so; do
    [ -e "$c" ] && { PROXY=$c; break; }
done
[ -n "${PROXY:-}" ] || { echo "sem p11-kit-proxy.so no host"; exit 1; }

banco_vazio "$FIREFOX"
banco_vazio "$SISTEMA"
instalar SoftHSM
subir

confere "antes de publicar, nada" "$(ancora nativos | grep -c 'não publicado (0 de 1 bancos)')" "2"
confere "antes de publicar, o host não vê o token" "$(no_host "$SISTEMA")" ""

ancora nativos publicar firefox >/dev/null
ancora nativos publicar sistema >/dev/null
confere "a verificação escolhe o proxy pelo caminho absoluto" \
    "$(grep -c "^library=$PROXY$" "$FIREFOX/pkcs11.txt" "$SISTEMA/pkcs11.txt" | cut -d: -f2 | tr '\n' ' ')" "1 1 "
confere "o sentinela sai ao terminar" "$(ls "$MODULES" | grep -c verificacao || true)" "0"
confere "o Firefox nativo vê o token uma vez" "$(no_host "$FIREFOX")" "Âncora (sistema)"
confere "o Chromium nativo vê o token uma vez" "$(no_host "$SISTEMA")" "Âncora (sistema)"
confere "e consta como publicado e verificado" "$(ancora nativos | grep -c ': publicado$')" "2"

no_sandbox=$(flatpak run --filesystem="$BANCADA" --command=python3 dev.lukakuuhaku.Ancora \
    -I "$OBRA/nss.py" "$SISTEMA" | python3 -I -c 'import json,sys; print(sum(1 for m,t in json.load(sys.stdin) if m.startswith("Âncora")))')
confere "dentro de um sandbox o mesmo banco ignora o registro" "$no_sandbox" "0"

ancora configurar org.chromium.Chromium >/dev/null
confere "a receita do Chromium Flatpak não apaga a publicação nativa" \
    "$(grep -c '^name=Âncora (sistema)$' "$SISTEMA/pkcs11.txt")" "1"
ancora desfazer org.chromium.Chromium

ancora nativos despublicar firefox >/dev/null
ancora nativos despublicar sistema >/dev/null
confere "despublicar tira o registro" "$(cat "$FIREFOX/pkcs11.txt" "$SISTEMA/pkcs11.txt" | grep -c 'Âncora' || true)" "0"
confere "e o host deixa de ver o token" "$(no_host "$SISTEMA")" ""

kill -TERM "$(pid_servico)"; sleep 3
remover SoftHSM
terminar
