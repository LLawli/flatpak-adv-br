#!/usr/bin/env bash
# Ponta a ponta da publicação, no sandbox de verdade.
#
# Instala duas extensões de teste (Driver.SoftHSM, com um token, e
# Driver.Travado, que trava ao listar slots), sobe `ancora servir` e confere,
# pelo p11-kit do host, o que fica publicado em cada situação. Tudo contra uma
# home falsa: a sessão de quem roda não é tocada.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

instalar SoftHSM Travado

echo "remote: unix:path=$SOCKS/morto" >"$MODULES/ancora-velho.module"
subir
confere "um servidor por driver no ar" "$(servidores)" "2"
confere "partida apaga o módulo de uma queda anterior e publica o SoftHSM" "$(publicados)" "ancora-softhsm.module"
confere "o host vê o token" "$(tokens_no_host)" "HUBTESTE"

sleep 40
confere "o driver travado nunca é publicado" "$(publicados)" "ancora-softhsm.module"
confere "o driver travado é parado" "$(grep -c 'Travado: parado' "$LOG")" "1"
confere "só o servidor do SoftHSM continua no ar" "$(servidores)" "1"
confere "o host segue vendo o token" "$(tokens_no_host)" "HUBTESTE"

set +e
flatpak run --command=ancora dev.lukakuuhaku.Ancora servir 2>/dev/null
segunda=$?
set -e
confere "uma segunda instância é recusada" "$segunda" "1"

kill -TERM "$(pid_servico)"; sleep 3
confere "SIGTERM despublica tudo" "$(publicados)" ""
confere "SIGTERM para os servidores" "$(servidores)" "0"

subir
confere "o servidor voltou" "$(servidores)" "2"
kill -KILL "$(pid_servico)"; sleep 3
confere "SIGKILL no serviço leva os servidores junto" "$(servidores)" "0"
confere "o módulo órfão não mostra token, e o host não trava" "$(tokens_no_host)" ""

subir
kill -TERM "$(pid_servico)"; sleep 3
confere "a partida seguinte limpa o que a queda deixou" "$(publicados)" ""

remover SoftHSM Travado
terminar
