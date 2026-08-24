#!/bin/sh
# Constrói e instala a extensão do PJeOffice na máquina de quem a executa.
#
#   curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/apps/instalar-pjeoffice.sh | sh
#
# Por que construir aqui, e não baixar pronto de um repositório: o PJeOffice é
# distribuído gratuitamente pelo CNJ, e gratuito não é o mesmo que
# redistribuível. Não há licença publicada que autorize terceiros a
# redistribuir os binários dele, e para obter o código-fonte é preciso ofício à
# Secretaria-Geral do CNJ. Um repositório Flatpak com o PJeOffice dentro seria
# redistribuição; um manifesto que a pessoa constrói na própria máquina, a
# partir do pacote publicado, não é. É a mesma regra que vale para os drivers
# proprietários deste projeto.
#
# Nada fica para trás: o que se baixa vai para um diretório temporário em disco
# (nunca em /tmp, que aqui é memória) e é apagado ao fim, dê certo ou não.
set -eu

RAMO=${ADVBR_RAMO:-main}
BASE=${ADVBR_BASE:-https://raw.githubusercontent.com/LLawli/flatpak-adv-br/$RAMO}
APP=dev.lukakuuhaku.AdvBr
EXTENSAO=$APP.App.PJeOffice

erro() { printf '\033[1;31m ✗\033[0m %s\n' "$*" >&2; exit 1; }
passo() { printf '\033[1;36m ▸\033[0m %s\n' "$*"; }

command -v flatpak >/dev/null || erro "o flatpak não está instalado."
command -v flatpak-builder >/dev/null ||
    erro "falta o flatpak-builder. Instale-o e rode de novo:
     Fedora: sudo dnf install flatpak-builder
     Debian: sudo apt install flatpak-builder
     Arch:   sudo pacman -S flatpak-builder"

# A extensão é construída CONTRA o aplicativo: sem ele não há o que estender, e
# o erro do flatpak-builder nesse caso não diz isso com todas as letras.
flatpak info --user "$APP" >/dev/null 2>&1 ||
    erro "o Certificado Digital ($APP) não está instalado. Instale-o antes."

# O SDK e a extensão de Java existem para construir, e não ficam no resultado.
passo "Conferindo o SDK (só na primeira vez, e é grande)"
flatpak install --user -y --noninteractive flathub \
    org.gnome.Sdk//50 org.freedesktop.Sdk.Extension.openjdk11//24.08 >/dev/null ||
    erro "não consegui instalar o SDK. Confira se o remoto flathub existe:
     flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo"

# Em disco, não em /tmp: /tmp é tmpfs em boa parte das distribuições atuais, e
# este build passa de 300 MB. Em memória isso vira swap, sem dono aparente.
TRABALHO=$(mktemp -d "${XDG_CACHE_HOME:-$HOME/.cache}/adv-br-pjeoffice.XXXXXX")
limpar() { rm -rf "$TRABALHO"; }
trap limpar EXIT INT TERM

passo "Baixando o manifesto"
for arquivo in dev.lukakuuhaku.AdvBr.App.PJeOffice.yml pjeoffice-pro-ui.sh; do
    curl -fsSL -o "$TRABALHO/$arquivo" "$BASE/apps/$arquivo" ||
        erro "não consegui baixar $arquivo de $BASE/apps/"
done

passo "Construindo (o pacote do CNJ é baixado agora, do servidor do CNJ)"
flatpak-builder --user --force-clean --install \
    "$TRABALHO/build" "$TRABALHO/dev.lukakuuhaku.AdvBr.App.PJeOffice.yml"

# Construir sem falhar não prova que instalou: o resultado é o que a janela do
# aplicativo procura para oferecer o botão de abrir.
flatpak info --user "$EXTENSAO" >/dev/null 2>&1 ||
    erro "o build terminou mas a extensão não está instalada."

printf '\n\033[1;32m ✓\033[0m PJeOffice instalado. Abra o Certificado Digital e use o botão Abrir.\n\n'
