#!/bin/bash
# Lançador dos assinadores (native messaging).
#
# O navegador executa este comando e conversa com ele por stdin/stdout, no
# protocolo de native messaging: 4 bytes de tamanho, little-endian, mais um
# JSON. NADA em stdout que não venha do assinador.
#
# Qual assinador é decidido pelo nome pelo qual este script foi chamado
# (adv-br-webpki, adv-br-websigner, adv-br-certisign), que é como se expõe um
# comando por assinador sem três cópias deste arquivo. O comando existe no
# pacote base mesmo sem a extensão correspondente instalada — é ele que dá a
# mensagem em vez de o navegador falhar sem explicação.
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

NOME=$(basename "$0")
CURTO=${NOME#adv-br-}

preparar_drivers >&2
registrar_modulos_no_sandbox
criar_atalhos

# O binário mora em <extensão>/bin/<curto>: a extensão do Lacuna instala
# bin/webpki, a do Softplan bin/websigner, a do Certisign bin/certisign.
ASSINADOR=""
for candidato in "$ASSINADORES"/*/bin/"$CURTO"; do
    [ -x "$candidato" ] && { ASSINADOR=$candidato; break; }
done

[ -n "$ASSINADOR" ] || {
    echo "adv-br: o assinador '$CURTO' não está instalado neste pacote." >&2
    echo "     ./instalar.sh --with-$CURTO" >&2
    exit 1
}

exec "$ASSINADOR" "$@"
