#!/bin/bash
# Imprime a série do p11-kit deste runtime, no formato "0.26".
#
# POR QUE ISTO EXISTE
#
# A ponte PKCS#11 deste projeto é o remoting do p11-kit: o host inicia um
# processo aqui dentro e conversa com ele por um pipe. O que trafega nesse pipe
# é a tabela de funções PKCS#11 serializada, e as duas pontas precisam
# concordar sobre ela.
#
# Quando não concordam, nada recusa a conexão. Medido com host em 0.26 e o
# outro lado em 0.25: os slots enumeram, o PIN é aceito, o C_FindObjects
# devolve as chaves, e TODO C_SignInit falha, com CKR_DEVICE_ERROR. Isso não
# atinge só assinar documento: a autenticação por certificado também exige uma
# assinatura, no CertificateVerify do handshake TLS, então o login por
# certificado para de funcionar com a lista de certificados aparecendo
# normalmente. É o modo de falha mais caro que existe aqui: tudo parece certo
# até o último passo.
#
# A causa está a montante, na 0.26.0 do p11-kit ("pkcs11: Update PKCS11 headers
# to version 3.2"), e não é acidente isolado: a 0.25.8 foi um "rpc: Unbreak
# protocol compatibility by reverting ...".
#
# Aqui o runtime é fixo, então quem varia é o host: um Debian trixie ou Ubuntu
# 24.04 traz p11-kit 0.25 e cai nesse caso. Daí o ./diagnostico.sh comparar.
#
# A série sai do próprio p11-kit, e não do nome do arquivo: o soname
# (libp11-kit.so.0.4.1 na 0.25, 0.4.10 e 0.4.11 na 0.26) distingue por acaso e
# ordena errado se comparado como texto.
set -euo pipefail

# Um laço, e não 'ls a b | head -1': com 'set -e' e 'pipefail', o ls que não
# acha um dos caminhos derruba o script antes da primeira linha útil, e o
# comando falha sem dizer nada, que é o pior jeito de falhar.
procurar_trust() { # <prefixo>
    local candidato
    for candidato in "$1"/lib/*/pkcs11/p11-kit-trust.so \
                     "$1"/lib/pkcs11/p11-kit-trust.so; do
        [ -e "$candidato" ] && { printf '%s\n' "$candidato"; return 0; }
    done
    return 1
}

# Quem responde é o par (binário, biblioteca) que a PONTE de fato usa. Quando o
# ./instalar.sh embutiu um p11-kit da série do host, é o dele; senão, o do
# runtime.
#
# E a pergunta tem de ser feita ao módulo de confiança DAQUELE p11-kit: o campo
# library-version que o list-modules imprime é a versão reportada pelo módulo,
# não a da libp11-kit do processo. Perguntar ao trust do runtime devolve 0.26
# mesmo com a ponte usando uma 0.25, o que parece certo e é justamente o
# engano que este comando existe para evitar.
COMPAT=/app/lib/p11kit-compat
if [ -x "$COMPAT/bin/p11-kit" ]; then
    P11KIT="$COMPAT/bin/p11-kit"
    export LD_LIBRARY_PATH="$COMPAT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    TRUST=$(procurar_trust "$COMPAT") || TRUST=""
else
    P11KIT=p11-kit
    TRUST=$(procurar_trust /usr) || TRUST=""
fi

[ -n "$TRUST" ] || { echo "adv-br: p11-kit-trust.so não encontrado." >&2; exit 1; }

# O Flatpak substitui o p11-kit-trust.module de todo sandbox pelo client dele,
# que reporta a versão do protocolo (1.1) e não a da biblioteca. Registrar o
# módulo de verdade, sob outro nome, é o que faz o p11-kit reportar a sua.
ARQUIVO=/etc/pkcs11/modules/zz-adv-br-serie.module
printf 'module: %s\ncritical: no\n' "$TRUST" > "$ARQUIVO" 2>/dev/null || {
    echo "adv-br: não consegui registrar um módulo para perguntar a versão." >&2
    exit 1
}

"$P11KIT" list-modules 2>/dev/null |
    sed -n '/^module: zz-adv-br-serie/,/^module:/p' |
    sed -n 's/^ *library-version: *//p' | head -1
