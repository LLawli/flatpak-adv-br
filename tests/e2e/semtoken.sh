#!/usr/bin/env bash
# Um driver sem nenhum slot (uma leitora sem cartão) está saudável: não pode
# ser tratado como travado, nem parado. Pelo RPC do p11-kit, pedir a lista com
# um buffer de tamanho zero volta CKR_BUFFER_TOO_SMALL, e uma sonda que tente
# de novo nesse caso fica em laço para sempre.
#
# Mas ele fica fora da publicação: no p11-kit-server.socket do host, o mesmo
# CKR_BUFFER_TOO_SMALL faz o iterador desistir, e um módulo sem token esconde
# todos os que vêm depois dele (src/supervisao.rs, `publicavel`).
set -euo pipefail
source "$(dirname "$0")/comum.sh"

instalar Vazio

# As sondas do driver: uma por driver, que fica (`ancora vigiar`).
sondas() {
    local c p n=0
    for c in /proc/[0-9]*/cmdline; do
        p=${c#/proc/}; p=${p%/cmdline}
        [[ "$(comando "$p")" == *"ancora vigiar "*"/Vazio/"* ]] && n=$((n + 1))
    done
    echo $n
}

situacao_vazio() {
    python3 -c 'import json, sys
print(next(d["situacao"]["tipo"] for d in json.load(open(sys.argv[1]))["drivers"] if d["extensao"] == "Vazio"))' "$SOCKS/estado.json"
}

subir
sleep 35
confere "o driver sem slot fica fora da publicação" "$(publicados | grep -o ancora-vazio.module || true)" ""
confere "e o estado diz sem token" "$(situacao_vazio)" "sem-token"
confere "e não é parado" "$(grep -c 'Vazio: parado' "$LOG" || true)" "0"
confere "nem dá erro" "$(grep -c 'Vazio: fora da publicação:' "$LOG" || true)" "0"
confere "uma sonda só, sem acumular" "$(sondas)" "1"
inicio=$(date +%s)
certificados=$(ancora certificados "$SOCKS/vazio" 2>&1 || true)
confere "os certificados de um driver sem slot são nenhum" "$certificados" "[]"
confere "e respondem na hora" "$(($(date +%s) - inicio < 5))" "1"

kill -TERM "$(pid_servico)"; sleep 3
remover Vazio
terminar
