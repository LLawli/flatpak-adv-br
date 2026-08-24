#!/bin/bash
# Lançador dos assinadores (native messaging).
#
# O navegador executa este comando e conversa com ele por stdin/stdout, no
# protocolo de native messaging: 4 bytes de tamanho, little-endian, mais um
# JSON. Vale a mesma regra do adv-br-pkcs11: NADA em stdout que não venha do
# assinador.
#
# Qual assinador é decidido pelo nome pelo qual este script foi chamado
# (adv-br-webpki, adv-br-websigner, adv-br-certisign), que é como se expõe um
# comando por assinador sem três cópias deste arquivo.
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

preparar_drivers
registrar_modulos_no_sandbox
criar_atalhos

case "$(basename "$0")" in
    adv-br-webpki)    ASSINADOR=/app/opt/lacuna-webpki/webpki ;;
    adv-br-websigner) ASSINADOR=/app/opt/softplan-websigner/websigner ;;
    adv-br-certisign)
        ASSINADOR=/app/opt/certisign-websigner/cswebsigner
        # O cswebsigner abre os .glade da interface por caminho absoluto em
        # /opt/certisign-websigner/res. A raiz do sandbox é um tmpfs gravável,
        # então o caminho que ele espera é criado aqui e desaparece com a
        # execução — mais honesto que remendar a string dentro do binário.
        mkdir -p /opt/certisign-websigner
        ln -sfn /app/opt/certisign-websigner/res /opt/certisign-websigner/res
        ;;
    *)
        echo "adv-br: não sei qual assinador é '$(basename "$0")'." >&2
        exit 2
        ;;
esac

[ -x "$ASSINADOR" ] || {
    echo "adv-br: $ASSINADOR não está instalado neste pacote." >&2
    exit 1
}

exec "$ASSINADOR" "$@"
