#!/bin/sh
# Executado pelo lançador antes do assinador subir. Ver assinadores/README.md.
#
# O cswebsigner abre os .glade da interface por caminho absoluto em
# /opt/certisign-websigner/res, e não há como redirecioná-lo. /usr pertence ao
# runtime e é somente leitura, mas a raiz do sandbox é um tmpfs gravável: o
# caminho que ele espera é criado aqui e desaparece com a execução — mais
# honesto que remendar a string dentro do binário.
set -eu

AQUI=$(cd -- "$(dirname -- "$0")" && pwd)

mkdir -p /opt/certisign-websigner
ln -sfn "$AQUI/res" /opt/certisign-websigner/res
