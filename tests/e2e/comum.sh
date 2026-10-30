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
# Os dados do Âncora (escolhas e registros), fora dos da sessão de quem roda.
DADOS=$BANCADA/dados
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

mkdir -p "$OBRA" "$MODULES" "$DADOS"
# Os módulos de hardware do sistema, desligados por nome na home falsa: o
# teste não pode disputar a leitora com o gpg/ssh-agent.
for m in opensc gnome-keyring; do
    if [ -f /usr/share/p11-kit/modules/$m.module ]; then
        lib=$(sed -n 's/^module: *//p' /usr/share/p11-kit/modules/$m.module)
        printf 'module: %s\nenable-in: nenhum-programa\n' "$lib" >"$MODULES/$m.module"
    fi
done

cp "$BANCADA/libsofthsm2.so" "$AQUI/travado.c" "$AQUI/espera.c" "$AQUI/nss.py" "$AQUI/assinador-teste.py" "$OBRA/"
printf 'contrato = 1\nnome = "SoftHSM de teste"\n[[pkcs11]]\nmodulo = "lib/libsofthsm2.so"\nambiente = { SOFTHSM2_CONF = "%s/softhsm2.conf" }\n' \
    "$BANCADA" >"$OBRA/softhsm.toml"
printf 'contrato = 1\nnome = "Driver que trava"\n[[pkcs11]]\nmodulo = "lib/libtravado.so"\n' \
    >"$OBRA/travado.toml"

construir() {
    local nome=$1 comandos=$2 fontes=$3 familia=${4:-Driver}
    cat >"$OBRA/dev.lukakuuhaku.Ancora.$familia.$nome.yml" <<EOF
id: dev.lukakuuhaku.Ancora.$familia.$nome
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
        "dev.lukakuuhaku.Ancora.$familia.$nome.yml" >"$OBRA/build-$nome.log" 2>&1)
}

# Instala as extensões de teste pedidas: SoftHSM, Travado, Espera (esta com
# ESPERA_COMPARTILHA=1 para listar tokens disputando a trava do login).
instalar() {
    local nome
    for nome in "$@"; do
        case $nome in
        SoftHSM) construir SoftHSM \
            '["install -Dm644 softhsm.toml ${FLATPAK_DEST}/ancora.toml", "install -Dm755 libsofthsm2.so ${FLATPAK_DEST}/lib/libsofthsm2.so"]' \
            '[{type: file, path: softhsm.toml}, {type: file, path: libsofthsm2.so}]' ;;
        Espera)
            printf 'contrato = 1\nnome = "Driver que espera"\n[[pkcs11]]\nmodulo = "lib/libespera.so"\nambiente = { ESPERA_COMPARTILHA = "%s", ESPERA_SEGUNDOS = "25" }\n' \
                "${ESPERA_COMPARTILHA:-0}" >"$OBRA/espera.toml"
            construir Espera \
            '["install -Dm644 espera.toml ${FLATPAK_DEST}/ancora.toml", "install -d ${FLATPAK_DEST}/lib", "gcc -shared -fPIC -Wall -Werror -I/usr/include/p11-kit-1 espera.c -o ${FLATPAK_DEST}/lib/libespera.so"]' \
            '[{type: file, path: espera.toml}, {type: file, path: espera.c}]' ;;
        Assinador)
            printf '{"name": "dev.lukakuuhaku.ancora.teste", "description": "teste", "path": "/opt/teste", "type": "stdio", "allowed_extensions": ["teste@ancora"]}\n' >"$OBRA/firefox.json"
            printf '{"name": "dev.lukakuuhaku.ancora.teste", "description": "teste", "path": "/opt/teste", "type": "stdio", "allowed_origins": ["chrome-extension://abcdefghijklmnopabcdefghijklmnop/"]}\n' >"$OBRA/chromium.json"
            printf 'contrato = 1\nnome = "Assinador de teste"\n[[assinador]]\nhost = "dev.lukakuuhaku.ancora.teste"\nexecutavel = "bin/assinador"\nmanifesto_firefox = "nm/firefox.json"\nmanifesto_chromium = "nm/chromium.json"\npkcs11 = ["libaetpkss.so.3"]\n' >"$OBRA/assinador.toml"
            construir Teste \
            '["install -Dm644 assinador.toml ${FLATPAK_DEST}/ancora.toml", "install -Dm755 assinador-teste.py ${FLATPAK_DEST}/bin/assinador", "install -Dm644 firefox.json ${FLATPAK_DEST}/nm/firefox.json", "install -Dm644 chromium.json ${FLATPAK_DEST}/nm/chromium.json"]' \
            '[{type: file, path: assinador.toml}, {type: file, path: assinador-teste.py}, {type: file, path: firefox.json}, {type: file, path: chromium.json}]' \
            Assinador ;;
        Travado) construir Travado \
            '["install -Dm644 travado.toml ${FLATPAK_DEST}/ancora.toml", "install -d ${FLATPAK_DEST}/lib", "gcc -shared -fPIC -Wall -Werror -I/usr/include/p11-kit-1 travado.c -o ${FLATPAK_DEST}/lib/libtravado.so"]' \
            '[{type: file, path: travado.toml}, {type: file, path: travado.c}]' ;;
        esac
    done
}

remover() {
    local refs=() nome
    for nome in "$@"; do
        case $nome in
        Assinador) refs+=("dev.lukakuuhaku.Ancora.Assinador.Teste") ;;
        *) refs+=("dev.lukakuuhaku.Ancora.Driver.$nome") ;;
        esac
    done
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
    local extra=()
    if [ -n "${ANCORA_SERVIDOR_DO_SISTEMA:-}" ]; then
        extra=(--env=ANCORA_SERVIDOR_DO_SISTEMA="$ANCORA_SERVIDOR_DO_SISTEMA"
            --filesystem="$(dirname "$ANCORA_SERVIDOR_DO_SISTEMA")")
    fi
    flatpak run --env=ANCORA_HOME_HOST="$FALSA" --env=ANCORA_DADOS="$DADOS" \
        --filesystem="$BANCADA" "${extra[@]}" --command=ancora dev.lukakuuhaku.Ancora "$@"
}

subir() {
    ancora servir >>"$LOG" 2>&1 &
    sleep 4
}

[ -z "$(pid_servico)" ] || { echo "já há um ancora servir rodando; pare-o antes"; exit 1; }
: >"$LOG"

# Processos que o teste sobe e que não podem sobrar se ele abortar no meio.
PARAR=()
limpar() {
    local p
    for p in "${PARAR[@]}"; do kill -TERM "$p" 2>/dev/null || true; done
    p=$(pid_servico)
    [ -z "$p" ] || kill -TERM "$p"
}
trap limpar EXIT
