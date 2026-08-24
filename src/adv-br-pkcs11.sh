#!/bin/bash
# A ponte PKCS#11 para fora: exporta um módulo deste pacote pelo stdin/stdout.
#
# Quem executa isto é o p11-kit do HOST, a partir de um arquivo .module escrito
# na home do usuário:
#
#     remote: |flatpak run --command=adv-br-pkcs11 io.github.llawli.AdvBr /app/lib/...
#
# O p11-kit-proxy do host inicia este comando sob demanda e conversa com ele
# pelo pipe. É o mesmo mecanismo que o 'sora provide pkcs11' usa para uma box
# do distrobox, com 'flatpak run' no lugar do 'distrobox enter'.
#
# É um processo por módulo, e não um só exportando o p11-kit-proxy do sandbox,
# por dois motivos. O primeiro é isolamento: um driver que derrube o processo
# que o carregou (o SafeNet faz isso ao encerrar, o SerproID mal preparado faz
# na carga) leva junto só a si mesmo. O segundo é que o Flatpak monta em todo
# sandbox um p11-kit-trust.module que não se consegue remover — o arquivo é um
# bind somente-leitura — e exportar o proxy devolveria ao host, como se fossem
# tokens, as âncoras de confiança que ele mesmo já tem.
#
# NÃO ESCREVA NADA EM STDOUT AQUI. O stdout é o protocolo RPC do p11-kit;
# qualquer byte a mais o corrompe. Mensagens vão para stderr.
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

MODULO=${1:-}
[ -n "$MODULO" ] || {
    echo "uso: adv-br-pkcs11 <caminho-do-modulo.so>" >&2
    echo "     os caminhos disponíveis saem de 'adv-br-modulos'." >&2
    exit 2
}

preparar_drivers

[ -e "$MODULO" ] || {
    echo "adv-br: módulo não encontrado neste pacote: $MODULO" >&2
    echo "     provavelmente a extensão de driver foi removida. Republique com" >&2
    echo "     ./host/publicar.sh" >&2
    exit 1
}

# Quando o p11-kit do host está numa série diferente da do runtime, o
# ./instalar.sh embute aqui um p11-kit da série dele. Só este processo — o da
# ponte — o usa: é ele que fala o protocolo com o outro lado. O resto do
# pacote continua com o p11-kit do runtime, que é o que os assinadores e o
# PJeOffice carregam, e que não atravessa pipe nenhum.
COMPAT=/app/lib/p11kit-compat
if [ -x "$COMPAT/bin/p11-kit" ]; then
    export LD_LIBRARY_PATH="$COMPAT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
    exec "$COMPAT/bin/p11-kit" remote "$MODULO"
fi

exec p11-kit remote "$MODULO"
