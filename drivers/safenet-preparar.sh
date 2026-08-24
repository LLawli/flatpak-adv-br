#!/bin/sh
# Executado pelo lançador antes da JVM subir. Ver drivers/README.md.
#
# A libeToken procura sua configuração em /etc/eToken.conf e
# /etc/eToken.common.conf por caminho absoluto, e não há como redirecioná-la.
# No sandbox /etc é um tmpfs recriado a cada execução, então os arquivos que
# vieram na extensão são copiados para lá agora.
set -eu

AQUI=$(dirname "$0")

for arquivo in eToken.conf eToken.common.conf; do
    [ -f "$AQUI/etc/$arquivo" ] || continue
    cp -f "$AQUI/etc/$arquivo" "/etc/$arquivo"
done

# A biblioteca guarda cache de token aqui e falha ao gravar se o diretório não
# existir. /var é gravável no sandbox.
mkdir -p /var/tmp/eToken.cache
