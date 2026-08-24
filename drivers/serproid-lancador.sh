#!/bin/sh
# Abre o aplicativo SerproID Desktop dentro do sandbox.
#
# É ele quem autentica na nuvem do Serpro e grava os certificados em
# ~/.config/serproid/certificados; a biblioteca PKCS#11 desta mesma extensão só
# lê esse diretório. Depois de associado, assinar não precisa dele aberto.
#
#   flatpak run --command=serproid io.github.llawli.AdvBr
set -eu

AQUI=$(cd -- "$(dirname -- "$0")/.." && pwd)

# O mesmo diretório que a biblioteca exige; ver preparar.sh.
mkdir -p "$HOME/.config/serproid/certificados"

# O aplicativo resolve tools/ e lib/ como caminhos relativos ao diretório de
# trabalho, então o cd não é conveniência.
cd "$AQUI/app"

exec ./jre/bin/java \
    -Djava.util.logging.config.class=smartcert.LogConfig \
    -classpath 'lib/*' \
    smartcert.Main "$@"
