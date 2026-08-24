#!/bin/bash
# Imprime "rótulo<TAB>caminho" de cada módulo PKCS#11 que este pacote enxerga.
#
# O publicador do host consome esta saída para escrever um .module por driver.
# Existe como comando, e não como tabela repetida no script do host, porque
# quem sabe quais extensões de driver estão instaladas é o pacote.
#     adv-br-modulos              rótulo<TAB>caminho de cada módulo
#     adv-br-modulos --contagem   registrados<TAB>carregados pelo p11-kit
set -euo pipefail
. /app/share/adv-br/comum-pkcs11.sh

if [ "${1:-}" = --contagem ]; then
    preparar_drivers >&2
    registrar_modulos_no_sandbox
    contar_modulos
    exit 0
fi

listar_modulos
