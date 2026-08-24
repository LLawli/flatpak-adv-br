#!/usr/bin/env bash
# Testes do repositório. Nenhum deles precisa de token espetado nem de leitora:
# o que exige hardware é ./diagnostico.sh e os host/testar-*.sh.
set -uo pipefail

RAIZ=$(cd "$(dirname "$0")/.." && pwd)
cd "$RAIZ" || exit 1

passou=0
falhou=0
ok()    { printf '\033[1;32m ✓\033[0m %s\n' "$*"; passou=$((passou + 1)); }
falha() { printf '\033[1;31m ✗\033[0m %s\n' "$*"; falhou=$((falhou + 1)); }
titulo() { printf '\n\033[1;36m━━ %s\033[0m\n' "$*"; }

# ---------------------------------------------------------------------------
titulo "Sintaxe"

for script in instalar.sh diagnostico.sh host/*.sh src/*.sh tests/*.sh; do
    [ -e "$script" ] || continue
    if bash -n "$script" 2>/dev/null; then ok "$script"; else falha "$script"; fi
done

for script in host/*.py tests/*.py; do
    [ -e "$script" ] || continue
    if python3 -c "import ast,sys; ast.parse(open(sys.argv[1]).read())" "$script"; then
        ok "$script"
    else
        falha "$script"
    fi
done

# ---------------------------------------------------------------------------
titulo "Manifestos"

for manifesto in io.github.llawli.AdvBr.yml drivers/*.yml assinadores/*.yml apps/*.yml; do
    [ -e "$manifesto" ] || continue
    if python3 - "$manifesto" <<'PY'
import sys
# Sem depender de PyYAML, que não é garantido: o que se confere aqui é o que
# quebra na prática: indentação com tabulação e um "id:" no topo.
texto = open(sys.argv[1], encoding="utf-8").read()
assert "\t" not in texto, "tabulação em YAML"
assert any(l.startswith("id:") for l in texto.splitlines()), "sem id:"
PY
    then ok "$manifesto"; else falha "$manifesto"; fi
done

# Cada arquivo citado como 'path:' precisa existir, ou o build falha só depois
# de baixar centenas de megabytes. Nos manifestos de extensão o caminho é
# relativo ao diretório do próprio manifesto.
for manifesto in io.github.llawli.AdvBr.yml drivers/*.yml assinadores/*.yml apps/*.yml; do
    [ -e "$manifesto" ] || continue
    base=$(dirname "$manifesto")
    while read -r caminho; do
        [ -n "$caminho" ] || continue
        if [ -e "$caminho" ] || [ -e "$base/$caminho" ]; then
            ok "fonte $caminho"
        else
            falha "fonte ausente: $caminho (citado em $manifesto)"
        fi
    done < <(sed -n 's/^ *path: *//p' "$manifesto")
done

# ---------------------------------------------------------------------------
titulo "nssdb.py"

BANCO=$(mktemp -d)
python3 - "$BANCO" <<'PY'
import ctypes, sys
nss = ctypes.CDLL("libnss3.so")
nss.NSS_Initialize.argtypes = [ctypes.c_char_p] * 4 + [ctypes.c_uint]
if nss.NSS_Initialize(("sql:" + sys.argv[1]).encode(), b"", b"", b"secmod.db", 0) != 0:
    raise SystemExit("NSS_Initialize falhou")
nss.NSS_Shutdown()
PY
if [ -e "$BANCO/pkcs11.txt" ]; then
    ok "banco NSS de teste criado sem certutil"

    python3 host/nssdb.py registrar "$BANCO" teste-adv-br /caminho/que/nao/existe.so
    if printf '%s\n' "$(python3 host/nssdb.py listar "$BANCO")" |
        grep -q '^teste-adv-br	'; then
        ok "registrar"
    else
        falha "registrar"
    fi

    # Idempotência: registrar duas vezes não pode duplicar a entrada.
    python3 host/nssdb.py registrar "$BANCO" teste-adv-br /caminho/que/nao/existe.so
    if [ "$(python3 host/nssdb.py listar "$BANCO" | grep -c '^teste-adv-br	')" = 1 ]; then
        ok "registrar é idempotente"
    else
        falha "registrar duplicou a entrada"
    fi

    python3 host/nssdb.py remover "$BANCO" teste-adv-br
    if printf '%s\n' "$(python3 host/nssdb.py listar "$BANCO")" |
        grep -q '^teste-adv-br	'; then
        falha "remover"
    else
        ok "remover"
    fi

    # O módulo interno do NSS não pode ser levado junto: sem ele o perfil perde
    # as chaves e os certificados.
    if printf '%s\n' "$(python3 host/nssdb.py listar "$BANCO")" |
        grep -q 'NSS Internal PKCS #11 Module'; then
        ok "o módulo interno do NSS ficou intacto"
    else
        falha "o módulo interno do NSS sumiu"
    fi
else
    falha "não consegui criar um banco NSS de teste (libnss3 ausente?)"
fi
rm -rf "$BANCO"

# ---------------------------------------------------------------------------
titulo "O pacote instalado"

APP_ID=io.github.llawli.AdvBr
if flatpak info --user "$APP_ID" >/dev/null 2>&1; then
    for comando in adv-br adv-br-pkcs11 adv-br-modulos adv-br-assinadores \
                   adv-br-ferramentas adv-br-atalhos adv-br-webpki adv-br-websigner \
                   adv-br-certisign adv-br-serie; do
        if flatpak run --command=sh "$APP_ID" -c "test -x /app/bin/$comando" 2>/dev/null; then
            ok "comando $comando"
        else
            falha "comando $comando ausente"
        fi
    done

    if [ "$(flatpak run --command=adv-br-modulos "$APP_ID" 2>/dev/null | wc -l)" -ge 1 ]; then
        ok "adv-br-modulos lista ao menos o OpenSC"
    else
        falha "adv-br-modulos não listou módulo nenhum"
    fi

    # A série do p11-kit precisa sair como "0.26", e não vazia: é ela que o
    # diagnóstico compara com a do host para pegar o modo de falha que
    # autentica e não assina.
    if printf '%s\n' "$(flatpak run --command=adv-br-serie "$APP_ID" 2>/dev/null)" |
        grep -qE '^[0-9]+\.[0-9]+$'; then
        ok "adv-br-serie responde com uma série de p11-kit"
    else
        falha "adv-br-serie não devolveu uma série"
    fi

    # Registrados e carregados têm de bater, e o piso absoluto é o que faz esta
    # guarda poder falhar: 0 e 0 passariam na comparação sozinha.
    contagem=$(flatpak run --command=adv-br-modulos "$APP_ID" --contagem 2>/dev/null | tr -d '\r')
    registrados=${contagem%%	*}
    carregados=${contagem##*	}
    if [ -n "$carregados" ] && [ "$carregados" -ge 2 ] && [ "$carregados" = "$registrados" ]; then
        ok "módulos registrados e carregados batem ($carregados)"
    else
        falha "contagem de módulos: $registrados registrados, $carregados carregados"
    fi

    # As ferramentas e os atalhos das extensões instaladas têm de aparecer: é
    # por eles que se abre o SerproID e o PJeOffice, que não são comandos do
    # pacote base.
    for extensao in App.PJeOffice Driver.SerproID; do
        flatpak info --user "$APP_ID.$extensao" >/dev/null 2>&1 || continue
        if printf '%s\n' "$(flatpak run --command=adv-br-ferramentas "$APP_ID" 2>/dev/null)" |
            grep -q "de ${extensao#*.}"; then
            ok "adv-br-ferramentas encontra a de $extensao"
        else
            falha "adv-br-ferramentas não encontrou a ferramenta de $extensao"
        fi
    done

    # Duas linhas por assinador instalado, uma por família de navegador. O
    # número não é fixo porque os assinadores são extensões: o que se confere é
    # a coerência entre o que está instalado e o que é descrito.
    instalados=$(flatpak list --columns=application 2>/dev/null |
        grep -c "^$APP_ID\.Assinador\." || true)
    descritos=$(flatpak run --command=adv-br-assinadores "$APP_ID" 2>/dev/null | wc -l)
    if [ "$descritos" = "$((instalados * 2))" ]; then
        ok "adv-br-assinadores descreve os $instalados assinador(es) instalado(s)"
    else
        falha "$instalados assinador(es) instalado(s), $descritos linha(s) descritas"
    fi
else
    printf '   (pacote não instalado; pulando)\n'
fi

# ---------------------------------------------------------------------------
printf '\n%d passaram, %d falharam\n\n' "$passou" "$falhou"
[ "$falhou" = 0 ]
