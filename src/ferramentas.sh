#!/bin/bash
# Executa uma ferramenta que veio junto com uma extensão de driver.
#
# Algumas extensões trazem mais que a biblioteca PKCS#11. O SerproID é o caso
# claro: ele é um aplicativo mais uma biblioteca, e sem abrir o aplicativo uma
# vez para associar o certificado não há o que assinar. Essas ferramentas ficam
# em <extensão>/bin/, e é daqui que se chega a elas:
#
#     flatpak run --command=adv-br-ferramentas io.github.llawli.AdvBr           # lista
#     flatpak run --command=adv-br-ferramentas io.github.llawli.AdvBr serproid  # executa
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

listar() {
    local driver achou=0
    for driver in "$DRIVERS"/*/bin/* "$APPS"/*/bin/*; do
        [ -x "$driver" ] || continue
        achou=1
        printf '  %s (de %s)\n' "$(basename "$driver")" \
            "$(basename "$(dirname "$(dirname "$driver")")")"
    done
    [ "$achou" = 1 ] ||
        printf '  nenhuma. As ferramentas vêm com as extensões de driver.\n'
}

if [ $# -eq 0 ]; then
    printf 'Ferramentas disponíveis:\n'
    listar
    exit 0
fi

NOME=$1
shift

preparar_drivers

for driver in "$DRIVERS"/*/bin/"$NOME" "$APPS"/*/bin/"$NOME"; do
    [ -x "$driver" ] || continue
    exec "$driver" "$@"
done

printf 'adv-br: ferramenta "%s" não encontrada.\n' "$NOME" >&2
listar >&2
exit 1
