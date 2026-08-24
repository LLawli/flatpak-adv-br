#!/bin/bash
# Os atalhos de menu que as extensões instaladas oferecem.
#
# Um .desktop dentro de uma extensão não é exportado pelo Flatpak: só o que
# está no aplicativo é, e no momento em que ele é construído. Uma extensão que
# a pessoa instala depois nunca passaria por lá. Então a travessia é explícita,
# e este comando é o lado de dentro dela: quem escreve no host é o
# ./host/publicar.sh.
#
# A convenção está em drivers/README.md: <extensão>/atalhos/<nome>.desktop, com
# @EXEC@ e @ICONE@ para o host preencher, e <nome>.png ao lado.
#
#     adv-br-atalhos                 lista "nome<TAB>desktop<TAB>png"
#     adv-br-atalhos <nome> desktop  despeja o .desktop
#     adv-br-atalhos <nome> icone    despeja o PNG
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

listar() {
    local atalho nome
    for atalho in "$DRIVERS"/*/atalhos/*.desktop "$APPS"/*/atalhos/*.desktop; do
        [ -e "$atalho" ] || continue
        nome=$(basename "$atalho" .desktop)
        printf '%s\t%s\t%s\n' "$nome" "$atalho" "${atalho%.desktop}.png"
    done
}

[ $# -eq 0 ] && { listar; exit 0; }

NOME=$1
QUAL=${2:-desktop}

while IFS=$'\t' read -r nome desktop png; do
    [ "$nome" = "$NOME" ] || continue
    case $QUAL in
        desktop) exec cat "$desktop" ;;
        icone)   [ -e "$png" ] && exec cat "$png"; exit 1 ;;
        *)       echo "adv-br-atalhos: use 'desktop' ou 'icone'." >&2; exit 2 ;;
    esac
done < <(listar)

echo "adv-br-atalhos: não há atalho chamado '$NOME'." >&2
exit 1
