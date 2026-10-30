# Partes comuns dos testes de ponta a ponta. Para `source`, não para rodar.
#
# Requer o Âncora instalado (--user) e uma bancada em $ANCORA_BANCADA com
# libsofthsm2.so e um softhsm2.conf cujo token se chama HUBTESTE. Ver
# tests/e2e/README.md.

BANCADA=${ANCORA_BANCADA:?defina ANCORA_BANCADA (ver tests/e2e/README.md)}
AQUI=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
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

terminar() {
    echo
    if [ "$falhas" -ne 0 ]; then
        echo "$falhas falha(s). Log do serviço:"
        cat "$LOG"
        exit 1
    fi
    echo "tudo certo"
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

cp "$BANCADA/libsofthsm2.so" "$AQUI/travado.c" "$AQUI/nss.py" "$OBRA/"
printf 'contrato = 1\nnome = "SoftHSM de teste"\n[[pkcs11]]\nmodulo = "lib/libsofthsm2.so"\nambiente = { SOFTHSM2_CONF = "%s/softhsm2.conf" }\n' \
    "$BANCADA" >"$OBRA/softhsm.toml"
printf 'contrato = 1\nnome = "Driver que trava"\n[[pkcs11]]\nmodulo = "lib/libtravado.so"\n' \
    >"$OBRA/travado.toml"

construir() {
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

# Instala as extensões de teste pedidas: SoftHSM, Travado.
instalar() {
    local nome
    for nome in "$@"; do
        case $nome in
        SoftHSM) construir SoftHSM \
            '["install -Dm644 softhsm.toml ${FLATPAK_DEST}/ancora.toml", "install -Dm755 libsofthsm2.so ${FLATPAK_DEST}/lib/libsofthsm2.so"]' \
            '[{type: file, path: softhsm.toml}, {type: file, path: libsofthsm2.so}]' ;;
        Travado) construir Travado \
            '["install -Dm644 travado.toml ${FLATPAK_DEST}/ancora.toml", "install -d ${FLATPAK_DEST}/lib", "gcc -shared -fPIC -Wall -Werror -I/usr/include/p11-kit-1 travado.c -o ${FLATPAK_DEST}/lib/libtravado.so"]' \
            '[{type: file, path: travado.toml}, {type: file, path: travado.c}]' ;;
        esac
    done
}

remover() {
    local refs=() nome
    for nome in "$@"; do refs+=("dev.lukakuuhaku.Ancora.Driver.$nome"); done
    flatpak uninstall --user --noninteractive -y "${refs[@]}" >/dev/null
}

# --- ferramentas ----------------------------------------------------------

tokens_no_host() {
    HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config python3 -I "$AQUI/listar.py" |
        python3 -I -c 'import json,sys; print(" ".join(t for t in json.load(sys.stdin).get("tokens", []) if t))'
}

publicados() { (cd "$MODULES" && ls ancora-*.module 2>/dev/null | tr '\n' ' ' | sed 's/ $//'); }

# A linha de comando de um processo, sem pipe: `tr | grep -q` com pipefail
# falha justamente quando acha, porque o grep sai cedo e o tr morre de SIGPIPE.
comando() { tr '\0' ' ' 2>/dev/null <"/proc/$1/cmdline"; }

servidores() {
    local c p n=0
    for c in /proc/[0-9]*/cmdline; do
        p=${c#/proc/}; p=${p%/cmdline}
        [[ "$(comando "$p")" == *"-n $SOCKS/"* ]] && n=$((n + 1))
    done
    echo $n
}

# O `ancora servir` desta bancada, pelo nome e pelo comando exato.
pid_servico() {
    local c p
    for c in /proc/[0-9]*/cmdline; do
        p=${c#/proc/}; p=${p%/cmdline}
        [ "$(cat /proc/"$p"/comm 2>/dev/null)" = ancora ] || continue
        [ "$(comando "$p")" = "ancora servir " ] && echo "$p"
    done
    return 0
}

# Roda um subcomando do Âncora contra a home falsa.
ancora() {
    flatpak run --env=ANCORA_HOME_HOST="$FALSA" --env=XDG_DATA_HOME="$BANCADA/dados" \
        --filesystem="$BANCADA" --command=ancora dev.lukakuuhaku.Ancora "$@"
}

subir() {
    ancora servir >>"$LOG" 2>&1 &
    sleep 4
}

[ -z "$(pid_servico)" ] || { echo "já há um ancora servir rodando; pare-o antes"; exit 1; }
: >"$LOG"
