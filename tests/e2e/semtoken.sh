#!/usr/bin/env bash
# Um driver sem nenhum slot (uma leitora sem cartão) tem de ser publicado sem
# token, e não tratado como travado. Pelo RPC do p11-kit, pedir a lista com
# um buffer de tamanho zero volta CKR_BUFFER_TOO_SMALL, e uma sonda que tente
# de novo nesse caso fica em laço para sempre.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

instalar Vazio

sondas() {
    local c p n=0
    for c in /proc/[0-9]*/cmdline; do
        p=${c#/proc/}; p=${p%/cmdline}
        [[ "$(comando "$p")" == *"ancora sondar $SOCKS/vazio"* ]] && n=$((n + 1))
    done
    echo $n
}

subir
sleep 35
confere "o driver sem slot é publicado" "$(publicados | grep -o ancora-vazio.module || true)" "ancora-vazio.module"
confere "e não é parado" "$(grep -c 'Vazio: parado' "$LOG" || true)" "0"
confere "nem sai da publicação" "$(grep -c 'Vazio: fora da publicação' "$LOG" || true)" "0"
confere "nenhuma sonda fica rodando" "$(sondas)" "0"
inicio=$(date +%s)
certificados=$(ancora certificados "$SOCKS/vazio" 2>&1 || true)
confere "os certificados de um driver sem slot são nenhum" "$certificados" "[]"
confere "e respondem na hora" "$(($(date +%s) - inicio < 5))" "1"

kill -TERM "$(pid_servico)"; sleep 3
remover Vazio
terminar
