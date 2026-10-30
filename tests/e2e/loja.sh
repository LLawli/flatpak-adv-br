#!/usr/bin/env bash
# Ponta a ponta das extensões pelos remotes: um remote local, de teste, com
# uma extensão exportada na hora. O Âncora, de dentro do sandbox, precisa
# achá-la, instalar, ver a versão nova, atualizar e remover, e um sandbox novo
# precisa montar o que foi instalado.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

REPO=$OBRA/repo
REMOTO=ancora-e2e
ID=dev.lukakuuhaku.Ancora.Driver.Loja

exportar() {
    local versao=$1
    printf 'contrato = 1\nnome = "Driver da loja"\nfabricante = "versão %s"\n[[pkcs11]]\nmodulo = "lib/libvazio.so"\n' \
        "$versao" >"$OBRA/loja.toml"
    cat >"$OBRA/$ID.metainfo.xml" <<XML
<?xml version="1.0" encoding="UTF-8"?>
<component type="addon">
  <id>$ID</id>
  <extends>dev.lukakuuhaku.Ancora</extends>
  <name>Driver da loja</name>
  <summary>Uma extensão de teste num remote local</summary>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>GPL-3.0-only</project_license>
</component>
XML
    cat >"$OBRA/$ID.yml" <<YML
id: $ID
runtime: dev.lukakuuhaku.Ancora
runtime-version: master
sdk: org.gnome.Sdk//51
build-extension: true
separate-locales: false
modules:
  - name: teste
    buildsystem: simple
    build-commands:
      - install -Dm644 loja.toml \${FLATPAK_DEST}/ancora.toml
      - install -d \${FLATPAK_DEST}/lib
      - gcc -shared -fPIC -Wall -Werror -I/usr/include/p11-kit-1 vazio.c -o \${FLATPAK_DEST}/lib/libvazio.so
      - install -Dm644 $ID.metainfo.xml \${FLATPAK_DEST}/share/metainfo/$ID.metainfo.xml
    sources:
      - {type: file, path: loja.toml}
      - {type: file, path: vazio.c}
      - {type: file, path: $ID.metainfo.xml}
YML
    (cd "$OBRA" && flatpak run org.flatpak.Builder --force-clean --disable-rofiles-fuse \
        --state-dir="$OBRA/estado" --repo="$REPO" "$OBRA/build-loja" "$ID.yml" >"$OBRA/build-loja.log" 2>&1)
    # Sem isto o remote não publica o nome e o resumo do metainfo.
    flatpak build-update-repo "$REPO" >/dev/null
}

limpar_loja() {
    flatpak uninstall --user --noninteractive -y "$ID" >/dev/null 2>&1 || true
    flatpak remote-delete --user --force "$REMOTO" >/dev/null 2>&1 || true
}
trap 'limpar_loja; limpar' EXIT

rm -rf "$REPO"
exportar 1
flatpak remote-add --user --no-gpg-verify --if-not-exists "$REMOTO" "file://$REPO"

linha() { ancora disponiveis 2>/dev/null | grep "^$ID:" || true; }
confere "o remote oferece a extensão, com o nome do metainfo" "$(linha)" "$ID: Driver da loja ($REMOTO)"

ancora instalar Driver.Loja >/dev/null
confere "instalar põe a extensão no host" "$(flatpak list --user --runtime --columns=application | grep -c "^$ID$")" "1"
confere "e ela consta como instalada" "$(linha)" "$ID: Driver da loja ($REMOTO) [instalada]"
confere "um sandbox novo a monta" "$(ancora extensoes | grep -c '^dev.lukakuuhaku.Ancora.Driver.Loja: Driver da loja (versão 1)$')" "1"

exportar 2
confere "a versão nova aparece como atualização" "$(linha)" "$ID: Driver da loja ($REMOTO) [atualização]"
ancora atualizar Driver.Loja >/dev/null
confere "atualizar traz a versão nova" "$(ancora extensoes | grep -c '(versão 2)$')" "1"
confere "e não sobra atualização" "$(linha)" "$ID: Driver da loja ($REMOTO) [instalada]"

ancora remover Driver.Loja >/dev/null
confere "remover tira a extensão do host" "$(flatpak list --user --runtime --columns=application | grep -c "^$ID$" || true)" "0"
confere "e ela volta a ser só oferecida" "$(linha)" "$ID: Driver da loja ($REMOTO)"
set +e
ancora instalar Driver.Inexistente >/dev/null 2>&1
inexistente=$?
ancora instalar AdvBr >/dev/null 2>&1
fora=$?
set -e
confere "instalar o que nenhum remote oferece falha" "$inexistente" "1"
confere "e um id fora do Âncora é recusado" "$fora" "1"

terminar
