#!/usr/bin/env bash
# Prova que o token atravessa do Flatpak até quem precisa dele.
#
# Duas travessias são medidas, e elas falham por motivos diferentes:
#
#   1. host → Flatpak       o p11-kit do host inicia 'flatpak run' e conversa
#                           pelo pipe. É o que o Firefox e o Chrome do host
#                           usam.
#   2. sandbox → host       um navegador em Flatpak fala com o socket do
#                           p11-kit do host, que por sua vez faz (1). Exige
#                           p11-kit-server.socket ligado e o override do
#                           navegador.
#
# Contar módulos não prova nada: todo sandbox já vem com um socket do p11-kit e
# um módulo de confiança. O que prova é chamar C_GetSlotList e ver um token com
# nome, que é o que tests/prova-pkcs11.py faz.
set -uo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/comum.sh"
PROVA="$(dirname "$AQUI")/tests/prova-pkcs11.py"

falhou=0

titulo "1 · Do host, pelo p11-kit"
PROXY=$(proxy_do_host) || erro "p11-kit-proxy.so não encontrado no host."
if saida=$(timeout 120 python3 "$PROVA" "$PROXY" 2>&1); then
    printf '%s\n' "$saida" | sed 's/^/   /'
    ok "o host enxerga token pelo proxy do p11-kit"
else
    printf '%s\n' "$saida" | sed 's/^/   /'
    aviso "nenhum token visível no host.
      Se o token está espetado, confira:
        - ./host/publicar.sh foi executado;
        - o pcscd do host está de pé (systemctl status pcscd.socket);
        - há driver para este token — só o OpenSC pode não bastar
          (./instalar.sh --with-safesign, --with-safenet)."
    falhou=1
fi

titulo "2 · De dentro de um navegador em Flatpak"
# O socket do host é o que o navegador em Flatpak alcança; o p11-kit-client.so
# do runtime dele é quem o consome. Aqui usamos um Flatpak qualquer que tenha
# o client.so no runtime — o que se está medindo é o encanamento, não o
# navegador.
if ! systemctl --user is-active p11-kit-server.socket >/dev/null 2>&1; then
    aviso "p11-kit-server.socket não está ativo; navegador em Flatpak não vai
      enxergar nada. Ligue com:
          systemctl --user enable --now p11-kit-server.socket
      ou rode ./host/publicar.sh --conceder"
    falhou=1
else
    CAIXA=${ADV_BR_CAIXA_DE_TESTE:-$APP_ID}
    TEMP=$(mktemp -d)
    cp "$PROVA" "$TEMP/prova.py"
    if saida=$(timeout 180 flatpak run \
        --filesystem=xdg-run/p11-kit/pkcs11 --filesystem="$TEMP:ro" \
        --command=sh "$CAIXA" -c '
            export P11_KIT_SERVER_ADDRESS=unix:path=$XDG_RUNTIME_DIR/p11-kit/pkcs11
            python3 '"$TEMP"'/prova.py /usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so
        ' 2>&1); then
        printf '%s\n' "$saida" | sed 's/^/   /'
        ok "um sandbox alcança o token pelo socket do host"
    else
        printf '%s\n' "$saida" | sed 's/^/   /'
        aviso "o sandbox não alcançou token nenhum pelo socket do host."
        falhou=1
    fi
    rm -rf "$TEMP"
fi

# ---------------------------------------------------------------------------
# Aplicativos que não são navegador e assinam com o token: eles leem o banco
# NSS do home real, e é por ele que a pergunta tem de passar. Provar que o
# módulo carrega não prova que o NSS o enxerga.
for consumidor in "${CONSUMIDORES[@]}"; do
    IFS='|' read -r id para_que <<<"$consumidor"
    flatpak_instalado "$id" || continue

    titulo "$id ($para_que)"
    if ! flatpak info --show-permissions "$id" 2>/dev/null |
        grep -q 'xdg-run/p11-kit/pkcs11'; then
        aviso "falta a permissão do socket:
      flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 $id"
        falhou=1
        continue
    fi

    TEMP=$(mktemp -d)
    cp "$(dirname "$AQUI")/tests/prova-nss.py" "$TEMP/prova-nss.py"
    if saida=$(timeout 180 flatpak run --filesystem="$TEMP:ro" --command=sh "$id" -c '
            python3 '"$TEMP"'/prova-nss.py "$HOME/.pki/nssdb"' 2>&1); then
        printf '%s\n' "$saida" | sed 's/^/   /'
        ok "o NSS de dentro do $id enxerga o token"
    else
        printf '%s\n' "$saida" | sed 's/^/   /'
        aviso "o NSS de dentro do $id não enxergou token nenhum.
      Rode ./host/publicar.sh e confira se o banco ~/.pki/nssdb existe (ele
      nasce na primeira vez que um navegador é aberto)."
        falhou=1
    fi
    rm -rf "$TEMP"
done

printf '\n'
exit "$falhou"
