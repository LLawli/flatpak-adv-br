#!/usr/bin/env bash
# A supervisão não pode confundir um token esperando a pessoa (um C_Login num
# leitor com teclado, segurando a transação do cartão) com um driver travado.
# Um aplicativo faz login no Driver.Espera, que demora 25 s; o serviço segue
# sondando. O login tem de terminar bem, e o driver não pode ser reiniciado.
#
# Duas variantes: listar tokens disputando a trava do login (como um driver
# que fala com o cartão para listar) e sem disputa.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

for compartilha in 1 0; do
    echo "== listar disputa a trava do login: $compartilha"
    ESPERA_COMPARTILHA=$compartilha instalar Espera
    : >"$LOG"
    subir
    confere "o driver está publicado" "$(publicados)" "ancora-espera.module"
    sleep 6 # a sonda seguinte cai no meio do login
    resultado=$(HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config timeout 90 python3 -I "$AQUI/entrar.py" || true)
    echo "     login: $resultado"
    sleep 12 # a sonda seguinte, já com o login terminado
    echo "     serviço:"; sed 's/^/       /' "$LOG"
    confere "o login termina bem" "$(echo "$resultado" | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("login_rv"))')" "0x0"
    confere "o login não foi interrompido" "$(echo "$resultado" | python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("segundos", 0) >= 24)')" "True"
    confere "o driver não foi reiniciado durante o login" "$(grep -c 'não respondeu' "$LOG" || true)" "0"
    confere "e volta publicado depois" "$(publicados)" "ancora-espera.module"
    kill -TERM "$(pid_servico)"; sleep 3
done

remover Espera
terminar
