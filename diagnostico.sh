#!/usr/bin/env bash
# Confere o encanamento inteiro, do token até o navegador, e diz o que fazer
# quando alguma peça falta.
#
# A ordem é a do caminho que o certificado percorre. Uma falha lá em cima
# explica todas as de baixo, então a primeira linha vermelha é a que interessa.
set -uo pipefail

. "$(cd "$(dirname "$0")" && pwd)/host/comum.sh"
RAIZ=$(cd "$(dirname "$0")" && pwd)

problemas=0
falha_conta() { falha "$@"; problemas=$((problemas + 1)); }
falha()  { printf '\033[1;31m ✗\033[0m %s\n' "$*"; }

# ---------------------------------------------------------------------------
titulo "1 · Host"

if command -v flatpak >/dev/null; then ok "flatpak"; else falha_conta "flatpak não instalado"; fi
if PROXY=$(proxy_do_host); then ok "p11-kit ($PROXY)"; else
    falha_conta "p11-kit-proxy.so não encontrado: navegador do host não alcança o token"
fi
if command -v python3 >/dev/null; then ok "python3"; else falha_conta "python3 não instalado"; fi

# As duas pontas da ponte precisam estar na mesma série do p11-kit. Quando não
# estão, nada recusa a conexão: os slots enumeram, o PIN é aceito, e toda
# assinatura falha, inclusive a do login por certificado, que assina no
# handshake TLS. É o modo de falha mais caro daqui.
SERIE_HOST=$(serie_p11kit_host)
if [ -n "$SERIE_HOST" ]; then
    ok "p11-kit do host na série $SERIE_HOST"
else
    aviso "não consegui descobrir a série do p11-kit do host."
fi

if [ -S /run/pcscd/pcscd.comm ] || systemctl is-active --quiet pcscd.socket 2>/dev/null; then
    ok "pcscd de pé"
else
    falha_conta "pcscd parado: nada fala com a leitora.  sudo systemctl enable --now pcscd.socket"
fi

if systemctl --user is-active --quiet p11-kit-server.socket 2>/dev/null; then
    ok "p11-kit-server.socket ativo (é o que navegador em Flatpak usa)"
else
    aviso "p11-kit-server.socket parado: navegador em Flatpak não enxerga token.
      systemctl --user enable --now p11-kit-server.socket"
fi

# ---------------------------------------------------------------------------
titulo "2 · O pacote"

if flatpak_instalado "$APP_ID"; then
    ok "$APP_ID instalado"
else
    falha_conta "$APP_ID não instalado.  ./instalar.sh"
    printf '\n'
    exit 1
fi

SERIE_PACOTE=$(flatpak run --command=adv-br-serie "$APP_ID" 2>/dev/null | tr -d '\r')
if [ -z "$SERIE_PACOTE" ]; then
    aviso "não consegui perguntar a série do p11-kit ao pacote. A comparação com a
      do host é justamente a que detecta o modo de falha mais caro daqui, então
      isto não é detalhe:
          flatpak run --command=adv-br-serie $APP_ID"
elif [ -n "$SERIE_HOST" ]; then
    if [ "$SERIE_PACOTE" = "$SERIE_HOST" ]; then
        ok "p11-kit do pacote na mesma série do host ($SERIE_PACOTE)"
    else
        falha_conta "o p11-kit do host está na série $SERIE_HOST e a ponte deste pacote
      na $SERIE_PACOTE. Assim, o token aparece no navegador, o PIN é aceito e
      TODA assinatura falha, inclusive a do login por certificado.
      O ./instalar.sh resolve: ele compila, dentro do pacote, um p11-kit da
      série do host. Rode-o de novo. Se ele disser que não conhece a série
      $SERIE_HOST, acrescente-a em packaging/p11kit-series.txt.
      (Os aplicativos deste pacote, como o PJeOffice e os assinadores, não
      passam pela ponte e funcionam de qualquer jeito.)"
    fi
fi

MODULOS=$(flatpak run --command=adv-br-modulos "$APP_ID" 2>/dev/null)
if [ -n "$MODULOS" ]; then
    while IFS=$'\t' read -r rotulo _; do
        [ -n "$rotulo" ] && ok "módulo: $rotulo"
    done <<<"$MODULOS"
else
    falha_conta "o pacote não tem módulo PKCS#11 nenhum."
fi

# Um driver registrado que não carrega desaparece da listagem em silêncio.
CONTAGEM=$(flatpak run --command=adv-br-modulos "$APP_ID" --contagem 2>/dev/null | tr -d '\r')
REGISTRADOS=${CONTAGEM%%	*}
CARREGADOS=${CONTAGEM##*	}
if [ -n "$REGISTRADOS" ] && [ -n "$CARREGADOS" ]; then
    # O piso absoluto é o que faz esta guarda poder falhar: comparar duas
    # contagens derivadas da mesma fonte passa feliz com 0 e 0.
    if [ "$CARREGADOS" -lt 2 ]; then
        falha_conta "o p11-kit do sandbox carregou $CARREGADOS módulo(s). Deveria ter ao
      menos o OpenSC do pacote e o módulo de confiança do Flatpak."
    elif [ "$CARREGADOS" -lt "$REGISTRADOS" ]; then
        falha_conta "$REGISTRADOS módulos registrados, $CARREGADOS carregados: algum driver
      não sobe. Não há erro porque todos são registrados com 'critical: no',
      o que denuncia é a diferença. Veja qual falta:
          flatpak run --command=sh $APP_ID -c 'p11-kit list-modules'"
    else
        ok "$CARREGADOS módulos registrados e carregados no sandbox"
    fi
fi

if [ "$(printf '%s\n' "$MODULOS" | wc -l)" = 1 ]; then
    aviso "só o OpenSC. Se o seu token não aparece abaixo, é provável que ele
      precise do driver do fabricante:
          ./instalar.sh --with-safesign    (GD Burti)
          ./instalar.sh --with-safenet     (eToken 5100, 5110, IDPrime)"
fi

# ---------------------------------------------------------------------------
titulo "3 · Publicação"

publicados=$(find "$MODULOS_HOST" -maxdepth 1 -name "$PREFIXO_MODULO*.module" 2>/dev/null | wc -l)
if [ "$publicados" -gt 0 ]; then
    ok "$publicados módulo(s) publicado(s) em $MODULOS_HOST"
else
    falha_conta "nada publicado.  ./host/publicar.sh"
fi

# Um .module publicado que aponte para um caminho que já não existe no pacote é
# o rastro de uma extensão de driver removida: o p11-kit tenta abri-lo em toda
# inicialização de navegador.
for arquivo in "$MODULOS_HOST/$PREFIXO_MODULO"*.module; do
    [ -e "$arquivo" ] || continue
    caminho=$(sed -n "s/^remote: .*$APP_ID //p" "$arquivo")
    [ -n "$caminho" ] || continue
    printf '%s\n' "$(printf '%s\n' "$MODULOS" | cut -f2)" | grep -qx "$caminho" ||
        falha_conta "$(basename "$arquivo") aponta para $caminho, que não existe mais
      no pacote.  ./host/publicar.sh"
done

# Cada família de navegador exige um campo diferente, e trocá-los é um erro
# mudo: o navegador ignora o manifesto sem dizer nada, e a extensão informa que
# o assinador não está instalado, mandando instalar o .deb que o Flatpak
# justamente substitui. Foi o que aconteceu, e é barato conferir.
declare -A MANIFESTO_VISTO=()
conferir_manifesto() {
    local id=$1 familia=$2 perfis=$3 manifestos=$4
    local dir arquivo campo
    case $familia in
        firefox)  campo=allowed_extensions ;;
        chromium) campo=allowed_origins ;;
        *)        return 0 ;;
    esac
    local certos erradas
    while read -r dir; do
        [ -d "$dir" ] || continue
        [ -n "${MANIFESTO_VISTO[$dir]:-}" ] && continue
        MANIFESTO_VISTO[$dir]=1
        certos=0
        erradas=""
        for arquivo in "$dir"/*.json; do
            [ -e "$arquivo" ] || continue
            grep -q "$PREFIXO_WRAPPER" "$arquivo" || continue
            if grep -q "\"$campo\"" "$arquivo"; then
                certos=$((certos + 1))
            else
                erradas="$erradas $(basename "$arquivo")"
            fi
        done
        # O 'path' de cada manifesto é o que o navegador vai executar. Para um
        # navegador em Flatpak, esse caminho tem de existir DENTRO do sandbox
        # dele, e é por isso que o atalho mora em ~/.var/app/<id>/data, cujo
        # caminho absoluto é o mesmo dos dois lados e portanto pode ser
        # conferido daqui. Um atalho que existe só para o host deixa a extensão
        # dizendo que o assinador não está instalado.
        for arquivo in "$dir"/*.json; do
            [ -e "$arquivo" ] || continue
            grep -q "$PREFIXO_WRAPPER" "$arquivo" || continue
            alvo=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("path",""))' \
                "$arquivo" 2>/dev/null)
            [ -n "$alvo" ] || continue
            [ -x "$alvo" ] ||
                falha_conta "$(basename "$arquivo") aponta para $alvo, que não existe
      ou não é executável.  ./host/publicar.sh"
        done

        if [ -n "$erradas" ]; then
            falha_conta "$dir:$erradas no formato da outra família de navegador
      (falta \"$campo\"). O navegador ignora o arquivo em silêncio e a extensão
      diz que o assinador não está instalado.  ./host/publicar.sh"
        elif [ "$certos" -gt 0 ]; then
            ok "$(printf '%s' "$dir" | sed "s|^$HOME|~|") ($certos manifesto(s) $familia)"
        fi
    done <<<"$manifestos"
    return 0
}
for navegador in "${NAVEGADORES[@]}"; do
    IFS='|' read -r id familia perfis manifestos <<<"$navegador"
    conferir_manifesto "" "$familia" \
        "$(expandir "$perfis" "$HOME" "${XDG_CONFIG_HOME:-$HOME/.config}")" \
        "$(expandir "$manifestos" "$HOME" "${XDG_CONFIG_HOME:-$HOME/.config}")"
    [ -d "$HOME/.var/app/$id" ] || continue
    conferir_manifesto "$id" "$familia" \
        "$(expandir "$perfis" "$HOME/.var/app/$id" "$HOME/.var/app/$id/config")" \
        "$(expandir "$manifestos" "$HOME/.var/app/$id" "$HOME/.var/app/$id/config")"
done

wrappers=$(find "$BIN_HOST" -maxdepth 1 -name "$PREFIXO_WRAPPER*" 2>/dev/null | wc -l)
if [ "$wrappers" -gt 0 ]; then
    ok "$wrappers atalho(s) de assinador em $BIN_HOST"
else
    aviso "nenhum atalho de assinador. Sem eles não se assina em navegador."
fi

# ---------------------------------------------------------------------------
titulo "4 · Bancos NSS"

conta_banco=0
declare -A BANCO_VISTO=()
conferir_banco() {
    local id=$1 familia=$2 perfis=$3
    local banco
    while read -r banco; do
        [ -n "$banco" ] || continue
        # A família Chromium inteira compartilha ~/.pki/nssdb.
        [ -n "${BANCO_VISTO[$banco]:-}" ] && continue
        BANCO_VISTO[$banco]=1
        conta_banco=$((conta_banco + 1))
        if printf '%s\n' "$(nss listar "$banco")" |
            grep -qE "^($NOME_NSS_HOST|$NOME_NSS_SANDBOX)	"; then
            ok "${id:-host}: $(basename "$banco")"
        else
            falha_conta "${id:-host}: $banco sem o módulo.  ./host/publicar.sh"
        fi
    done < <(bancos_nss "$perfis")
}
CONFIG_HOST="${XDG_CONFIG_HOME:-$HOME/.config}"
for navegador in "${NAVEGADORES[@]}"; do
    # O quarto campo (manifestos) não interessa aqui.
    IFS='|' read -r id familia perfis _ <<<"$navegador"
    conferir_banco "" "$familia" "$(expandir "$perfis" "$HOME" "$CONFIG_HOST")"
    [ -d "$HOME/.var/app/$id" ] || continue
    conferir_banco "$id" "$familia" \
        "$(expandir "$perfis" "$HOME/.var/app/$id" "$HOME/.var/app/$id/config")"
done
[ "$conta_banco" -gt 0 ] || aviso "nenhum banco NSS encontrado: nenhum navegador foi
      aberto ainda nesta máquina?"

# Resquício comum: o sora-adv-br publica com outros nomes e aponta para uma box
# do distrobox. Com a box removida, o módulo continua registrado e o p11-kit
# tenta iniciá-la a cada abertura de navegador.
if printf '%s\n' "$(nss listar "$HOME/.pki/nssdb" 2>/dev/null)" | grep -q '^sora-'; then
    aviso "há módulos do sora-adv-br registrados em ~/.pki/nssdb. Se você não usa
      mais a box do distrobox, eles só custam tempo em cada abertura:
          python3 host/nssdb.py remover ~/.pki/nssdb sora-p11-kit-proxy"
fi

# ---------------------------------------------------------------------------
titulo "5 · Travessia"

if "$RAIZ/host/testar-pkcs11.sh" >/dev/null 2>&1; then
    ok "o token atravessa até o host e até um sandbox"
else
    falha_conta "a travessia falhou. Rode ./host/testar-pkcs11.sh para ver onde."
fi

# ---------------------------------------------------------------------------
titulo "6 · Quanto custa cada driver"

# Um driver lento não é detalhe: o navegador enumera os slots ao abrir, e
# espera. O SafeNet, sem token SafeNet espetado, leva mais de um minuto dentro
# do sandbox. Medido, e não só aqui: acontece igual em outro Flatpak, com
# outro pacote, e não acontece com a mesma biblioteca no host.
TEMP=$(mktemp -d)
cp "$RAIZ/tests/prova-pkcs11.py" "$TEMP/prova.py"
while IFS=$'\t' read -r rotulo biblioteca; do
    [ -n "$rotulo" ] || continue
    inicio=$(date +%s)
    timeout 90 flatpak run --filesystem="$TEMP:ro" --command=sh "$APP_ID" -c "
        . /app/share/adv-br/comum-pkcs11.sh
        preparar_drivers >/dev/null 2>&1
        python3 $TEMP/prova.py '$biblioteca'" >/dev/null 2>&1
    gasto=$(( $(date +%s) - inicio ))
    if [ "$gasto" -ge 20 ]; then
        aviso "$rotulo levou ${gasto}s para enumerar. É esse tempo que o navegador
      espera ao abrir. Se você não usa o token deste driver, remova-o:
          flatpak uninstall --user $APP_ID.Driver.<Nome>
          ./host/publicar.sh"
    else
        ok "$rotulo: ${gasto}s"
    fi
done <<<"$MODULOS"
rm -rf "$TEMP"

# ---------------------------------------------------------------------------
titulo "7 · Alguma leitora travada?"

# Uma leitora que existe e não responde apaga da lista TODOS os certificados,
# e não só o dela. A pergunta que todo programa faz é
# C_GetSlotList(CKF_TOKEN_PRESENT), e um único CKR_DEVICE_ERROR reprova a
# chamada inteira — some junto o certificado em nuvem, que não tem leitora
# nenhuma e nada tem com isso.
#
# Engana porque a linha de comando desmente: pkcs11-tool -L pergunta SEM o
# filtro e mostra tudo. Então parece que está tudo bem, enquanto o navegador, o
# Papers e o PJeOffice não mostram nada.
TEMP=$(mktemp -d)
cp "$RAIZ/tests/prova-leitora.py" "$TEMP/leitora.py"
saida=$(timeout 120 flatpak run --filesystem="$TEMP:ro" --command=sh "$APP_ID" -c "
    . /app/share/adv-br/comum-pkcs11.sh
    preparar_drivers >/dev/null 2>&1
    python3 $TEMP/leitora.py" 2>&1)
codigo=$?
rm -rf "$TEMP"
case $codigo in
    0) ok "nenhuma leitora travada ($saida)" ;;
    1) aviso "$saida
      Uma leitora travada esconde TODOS os tokens de quem pergunta do jeito
      normal: navegador, Papers, PJeOffice, assinadores. A causa mais comum é o
      gnupg segurando o cartão, no caso de quem usa a mesma YubiKey para
      assinar commit e para certificado. Solte-o com:
          gpgconf --kill scdaemon
      E para não repetir a cada assinatura, acrescente ao ~/.gnupg/scdaemon.conf:
          card-timeout 1" ;;
    *) aviso "não deu para perguntar ao p11-kit: $saida" ;;
esac

# ---------------------------------------------------------------------------
printf '\n'
if [ "$problemas" = 0 ]; then
    printf '\033[1;32m ✓ tudo no lugar.\033[0m\n\n'
else
    printf '\033[1;31m ✗ %d problema(s).\033[0m Comece pelo primeiro: os de baixo costumam ser efeito dele.\n\n' "$problemas"
fi
exit "$problemas"
