#!/usr/bin/env bash
# Ponta a ponta da publicação, no sandbox de verdade.
#
# Constrói duas extensões de teste (Driver.SoftHSM, com um token, e
# Driver.Travado, que trava ao listar slots), sobe `ancora servir` e confere,
# pelo p11-kit do host, o que fica publicado em cada situação. Tudo contra uma
# home falsa: a sessão de quem roda não é tocada.
#
# Requer o Âncora instalado (--user) e uma bancada em $ANCORA_BANCADA com
# libsofthsm2.so e um softhsm2.conf cujo token se chama HUBTESTE. Ver
# tests/e2e/README.md.
set -euo pipefail

BANCADA=${ANCORA_BANCADA:?defina ANCORA_BANCADA (ver tests/e2e/README.md)}
AQUI=$(cd "$(dirname "$0")" && pwd)
OBRA=$BANCADA/obra
FALSA=$BANCADA/home-falsa
MODULES=$FALSA/.config/pkcs11/modules
SOCKS=/run/user/$(id -u)/ancora
LOG=$BANCADA/servico.log
falhas=0

confere() {
    if [ "$2" = "$3" ]; then
        echo "ok   $1"
    else
        echo "FALHA $1: esperado [$3], obtido [$2]"
        falhas=$((falhas + 1))
    fi
}

# --- preparo --------------------------------------------------------------

mkdir -p "$OBRA" "$MODULES"
# Os módulos de hardware do sistema, desligados por nome na home falsa: o
# teste não pode disputar a leitora com o gpg/ssh-agent.
for m in opensc gnome-keyring; do
    if [ -f /usr/share/p11-kit/modules/$m.module ]; then
        lib=$(sed -n 's/^module: *//p' /usr/share/p11-kit/modules/$m.module)
        printf 'module: %s\nenable-in: nenhum-programa\n' "$lib" >"$MODULES/$m.module"
    fi
done

cp "$BANCADA/libsofthsm2.so" "$AQUI/travado.c" "$OBRA/"
printf 'contrato = 1\nnome = "SoftHSM de teste"\n[[pkcs11]]\nmodulo = "lib/libsofthsm2.so"\nambiente = { SOFTHSM2_CONF = "%s/softhsm2.conf" }\n' \
    "$BANCADA" >"$OBRA/softhsm.toml"
printf 'contrato = 1\nnome = "Driver que trava"\n[[pkcs11]]\nmodulo = "lib/libtravado.so"\n' \
    >"$OBRA/travado.toml"

extensao() {
    local nome=$1 comandos=$2 fontes=$3
    cat >"$OBRA/dev.lukakuuhaku.Ancora.Driver.$nome.yml" <<EOF
id: dev.lukakuuhaku.Ancora.Driver.$nome
runtime: dev.lukakuuhaku.Ancora
runtime-version: master
sdk: org.gnome.Sdk//51
build-extension: true
separate-locales: false
appstream-compose: false
modules:
  - name: teste
    buildsystem: simple
    build-commands: $comandos
    sources: $fontes
EOF
    (cd "$OBRA" && flatpak run org.flatpak.Builder --user --install --force-clean \
        --disable-rofiles-fuse --state-dir="$OBRA/estado" "$OBRA/build-$nome" \
        "dev.lukakuuhaku.Ancora.Driver.$nome.yml" >"$OBRA/build-$nome.log" 2>&1)
}
extensao SoftHSM \
    '["install -Dm644 softhsm.toml ${FLATPAK_DEST}/ancora.toml", "install -Dm755 libsofthsm2.so ${FLATPAK_DEST}/lib/libsofthsm2.so"]' \
    '[{type: file, path: softhsm.toml}, {type: file, path: libsofthsm2.so}]'
extensao Travado \
    '["install -Dm644 travado.toml ${FLATPAK_DEST}/ancora.toml", "install -d ${FLATPAK_DEST}/lib", "gcc -shared -fPIC -Wall -Werror -I/usr/include/p11-kit-1 travado.c -o ${FLATPAK_DEST}/lib/libtravado.so"]' \
    '[{type: file, path: travado.toml}, {type: file, path: travado.c}]'

# --- ferramentas ----------------------------------------------------------

tokens_no_host() {
    HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config python3 -I "$AQUI/listar.py" |
        python3 -I -c 'import json,sys; print(" ".join(t for t in json.load(sys.stdin).get("tokens", []) if t))'
}
publicados() { (cd "$MODULES" && ls ancora-*.module 2>/dev/null | tr '\n' ' ' | sed 's/ $//'); }
servidores() {
    local c n=0
    for c in /proc/[0-9]*/cmdline; do
        tr '\0' ' ' 2>/dev/null <"$c" | grep -q -- "-n $SOCKS/" && n=$((n + 1))
    done
    echo $n
}
# O `ancora servir` desta bancada, pelo nome e pelo comando exato.
pid_servico() {
    local c p
    for c in /proc/[0-9]*/cmdline; do
        p=${c#/proc/}; p=${p%/cmdline}
        [ "$(cat /proc/"$p"/comm 2>/dev/null)" = ancora ] || continue
        [ "$(tr '\0' ' ' 2>/dev/null <"$c")" = "ancora servir " ] && echo "$p"
    done
}
subir() {
    flatpak run --env=ANCORA_DIR_MODULES="$MODULES" --filesystem="$BANCADA" \
        --command=ancora dev.lukakuuhaku.Ancora servir >>"$LOG" 2>&1 &
    sleep 4
}

[ -z "$(pid_servico)" ] || { echo "já há um ancora servir rodando; pare-o antes"; exit 1; }

# --- cenários -------------------------------------------------------------

: >"$LOG"
echo "remote: unix:path=$SOCKS/morto" >"$MODULES/ancora-velho.module"
subir
confere "partida apaga o módulo de uma queda anterior e publica o SoftHSM" "$(publicados)" "ancora-softhsm.module"
confere "o host vê o token" "$(tokens_no_host)" "HUBTESTE"

sleep 40
confere "o driver travado nunca é publicado" "$(publicados)" "ancora-softhsm.module"
confere "o driver travado é parado" "$(grep -c 'Travado: parado' "$LOG")" "1"
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
kill -KILL "$(pid_servico)"; sleep 3
confere "SIGKILL no serviço leva os servidores junto" "$(servidores)" "0"
confere "o módulo órfão não mostra token, e o host não trava" "$(tokens_no_host)" ""

subir
kill -TERM "$(pid_servico)"; sleep 3
confere "a partida seguinte limpa o que a queda deixou" "$(publicados)" ""

flatpak uninstall --user --noninteractive -y \
    dev.lukakuuhaku.Ancora.Driver.SoftHSM dev.lukakuuhaku.Ancora.Driver.Travado >/dev/null

echo
if [ "$falhas" -ne 0 ]; then
    echo "$falhas falha(s). Log do serviço:"
    cat "$LOG"
    exit 1
fi
echo "tudo certo"
