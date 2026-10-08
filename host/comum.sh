#!/usr/bin/env bash
# Funções compartilhadas pelos scripts do host. Carregado com "."; não roda
# sozinho.
#
# shellcheck disable=SC2034
# (as variáveis daqui são usadas por quem carrega este arquivo, não por ele.)

APP_ID=${ADV_BR_APP_ID:-io.github.llawli.AdvBr}
AQUI=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

titulo() { printf '\n\033[1;36m━━ %s\033[0m\n' "$*"; }
log()    { printf '\033[1;34m::\033[0m %s\n' "$*"; }
ok()     { printf '\033[1;32m ✓\033[0m %s\n' "$*"; }
aviso()  { printf '\033[1;33m !\033[0m %s\n' "$*" >&2; }
erro()   { printf '\033[1;31m ✗\033[0m %s\n' "$*" >&2; exit 1; }

# Onde o p11-kit do host procura módulos do usuário, e o prefixo com que este
# projeto nomeia os seus. O prefixo é o que permite remover exatamente o que
# foi publicado, sem tocar em módulo de outro programa.
MODULOS_HOST="${XDG_CONFIG_HOME:-$HOME/.config}/pkcs11/modules"
PREFIXO_MODULO=adv-br-

# Os programas que nunca carregam os drivers publicados. Os serviços da sessão
# gráfica carregam todo .module do usuário na subida, e cada módulo nosso vira
# um "flatpak run" que depende da sessão já estar de pé: no GNOME isso travou o
# login. A lista é a mesma de FORA_DA_SESSAO em ui/publicador.py, e o
# tests/testar.sh confere que as duas não divergem.
FORA_DA_SESSAO="gsd-smartcard, gnome-software, gvfsd-http, csd-smartcard, gnome-shell, gnome-keyring-daemon"
MARCA_RESTRICAO="# Os serviços da sessão gráfica não carregam este driver."

# As linhas enable-in/disable-in que um .module deve levar: as que a pessoa
# escreveu, se editou o arquivo, ou a nossa. A nossa é a que vem logo depois
# da MARCA_RESTRICAO; qualquer outra é dela, e sobrevive à republicação.
restricoes_do_modulo() {
    local arquivo=$1 proprias=""
    if [ -f "$arquivo" ]; then
        proprias=$(awk -v marca="$MARCA_RESTRICAO" '
            /^(enable|disable)-in:/ && anterior != marca { print }
            { anterior = $0 }' "$arquivo")
    fi
    if [ -n "$proprias" ]; then
        printf '%s\n' "$proprias"
    else
        printf '%s\ndisable-in: %s\n' "$MARCA_RESTRICAO" "$FORA_DA_SESSAO"
    fi
}

# Wrappers de native messaging, e o nome com que os módulos aparecem no banco
# NSS de cada navegador.
BIN_HOST="$HOME/.local/bin"
PREFIXO_WRAPPER=adv-br-

# Onde fica o atalho de um navegador em Flatpak, RELATIVO a ~/.var/app/<id>.
#
# Não é .local/bin: um sandbox não enxerga isso. O Flatpak monta, de todo
# ~/.var/app/<id>, só os diretórios XDG (cache, config, data) e o que o app
# declarar como 'persistent': o Firefox declara .mozilla, e mais nada. Um
# atalho em ~/.var/app/<id>/.local/bin existe para o host e não existe para o
# aplicativo, e o sintoma é o navegador dizer que o assinador não está
# instalado.
#
# 'data' tem uma propriedade que resolve o resto: o caminho absoluto
# ~/.var/app/<id>/data/... é o MESMO dentro e fora do sandbox. Assim o que se
# grava no manifesto vale nos dois lados, e o ./diagnostico.sh consegue
# conferir do host se o arquivo existe.
SUBDIR_ATALHO_FLATPAK=data/adv-br
NOME_NSS_HOST=adv-br
NOME_NSS_SANDBOX=adv-br-p11-kit-client

# Navegadores conhecidos.
#
#   id-flatpak | família | perfis NSS | manifestos de native messaging
#
# @HOME@ e @CONFIG@ são a home e o XDG_CONFIG_HOME de onde o navegador está:
# para o do host, $HOME e ~/.config; para o mesmo navegador em Flatpak,
# ~/.var/app/<id> e ~/.var/app/<id>/config. É a mesma tabela para os dois casos
# porque, do ponto de vista do navegador, os caminhos são os mesmos: o que
# muda é onde essa home está montada.
#
# Firefox 147 moveu o perfil para $XDG_CONFIG_HOME/mozilla/firefox e deixou o
# native messaging onde estava, em ~/.mozilla (bugzilla 2005167). Por isso os
# dois caminhos de perfil, e por isso o de manifesto não acompanha.
NAVEGADORES=(
    "org.mozilla.firefox|firefox|@CONFIG@/mozilla/firefox,@HOME@/.mozilla/firefox|@HOME@/.mozilla/native-messaging-hosts"
    "io.gitlab.librewolf-community|firefox|@CONFIG@/librewolf,@HOME@/.librewolf|@HOME@/.librewolf/native-messaging-hosts"
    "one.ablaze.floorp|firefox|@CONFIG@/floorp,@HOME@/.floorp|@HOME@/.floorp/native-messaging-hosts"
    "net.waterfox.waterfox|firefox|@CONFIG@/waterfox,@HOME@/.waterfox|@HOME@/.waterfox/native-messaging-hosts"
    "org.mozilla.Thunderbird|firefox|@CONFIG@/thunderbird,@HOME@/.thunderbird|@HOME@/.thunderbird/native-messaging-hosts"
    "com.google.Chrome|chromium|@HOME@/.pki/nssdb|@CONFIG@/google-chrome/NativeMessagingHosts"
    "org.chromium.Chromium|chromium|@HOME@/.pki/nssdb|@CONFIG@/chromium/NativeMessagingHosts"
    "com.brave.Browser|chromium|@HOME@/.pki/nssdb|@CONFIG@/BraveSoftware/Brave-Browser/NativeMessagingHosts"
    "com.vivaldi.Vivaldi|chromium|@HOME@/.pki/nssdb|@CONFIG@/vivaldi/NativeMessagingHosts"
    "com.microsoft.Edge|chromium|@HOME@/.pki/nssdb|@CONFIG@/microsoft-edge/NativeMessagingHosts"
    "com.opera.Opera|chromium|@HOME@/.pki/nssdb|@CONFIG@/opera/NativeMessagingHosts"
)

# Substitui @HOME@ e @CONFIG@ numa lista separada por vírgulas, e imprime um
# caminho por linha.
expandir() {
    local padroes=$1 casa=$2 config=$3
    printf '%s' "$padroes" | tr ',' '\n' |
        sed -e "s|@HOME@|$casa|g" -e "s|@CONFIG@|$config|g"
}

# Bancos NSS existentes sob os caminhos dados. Um banco é um diretório com
# cert9.db: para o Firefox, um por perfil; para a família Chromium, um só.
#
# Procurar o arquivo, em vez de deduzir o layout, é o que sobrevive ao Firefox
# mudar o perfil de lugar e aos forks que usam outro nome de diretório.
bancos_nss() {
    local raizes=$1 raiz
    while read -r raiz; do
        [ -d "$raiz" ] || continue
        # -maxdepth 2: o próprio diretório (Chromium) ou um nível de perfis
        # (Firefox). Mais fundo que isso é backup ou lixo.
        find "$raiz" -maxdepth 2 -name cert9.db -printf '%h\n' 2>/dev/null
    done <<<"$raizes" | sort -u
}

# O p11-kit-proxy do host, procurado e não deduzido: em Fedora ele está em
# /usr/lib64, em Debian e Arch em /usr/lib/<triplet>.
#
# Não confunda com p11-kit-client.so: esse o Fedora nem empacota, e ele é peça
# do lado de dentro de um sandbox, não do host.
proxy_do_host() {
    local candidato
    for candidato in /usr/lib64/p11-kit-proxy.so /usr/lib/p11-kit-proxy.so \
                     /usr/lib/*/p11-kit-proxy.so; do
        [ -e "$candidato" ] && { printf '%s\n' "$candidato"; return 0; }
    done
    return 1
}

# O p11-kit-client.so dentro do sandbox. É caminho de lá, não daqui: o Fedora
# nem empacota esse arquivo.
CLIENT_NO_SANDBOX=/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so

# Aplicativos em Flatpak que não são navegador e que também precisam do token:
# eles leem o banco NSS do home REAL (têm --filesystem=home), e não um banco
# próprio em ~/.var/app. É por isso que os bancos do host recebem os dois
# registros: o do proxy, para quem roda no host, e o do client, para quem lê o
# mesmo arquivo de dentro de um sandbox. Um registro que não resolve é ignorado
# em silêncio pelo NSS, então os dois convivem.
#
#   id-flatpak | para que serve
CONSUMIDORES=(
    "org.gnome.Papers|assinar e validar PDF"
    "org.gnome.Evince|assinar e validar PDF"
    "org.libreoffice.LibreOffice|assinar documento"
    "org.kde.okular|assinar e validar PDF"
)

flatpak_instalado() {
    flatpak info --user "$1" >/dev/null 2>&1 || flatpak info "$1" >/dev/null 2>&1
}

# O Assinador Serpro é a terceira maneira de um programa procurar um token, e
# não se parece com nenhuma das outras duas: ele não lê banco NSS e não fala
# com o p11-kit. Ele tem uma lista própria de bibliotecas PKCS#11, em
# ~/.signer/drivers.properties, uma por linha, no formato <apelido>=<caminho>.
# Fora dessa lista há só o que o Serpro embutiu de fábrica, e por isso um token
# publicado por este projeto não aparecia na tela dele.
#
# O que se escreve ali é o p11-kit-proxy do host, e não um driver. O proxy já
# lê os .module escritos pelo ./host/publicar.sh, então esta linha só responde
# por todos os drivers instalados aqui, inclusive os que forem instalados
# depois: a lista do Serpro nunca precisa ser mexida de novo.
LISTA_SERPRO="$HOME/.signer/drivers.properties"
CHAVE_LISTA=advbr-p11-kit

# O arquivo é de quem instalou o Assinador Serpro, não nosso: pode ter linha de
# outro driver, posta à mão. Por isso tudo o que se faz nele é por chave, e a
# chave é esta. O que não for dela fica onde está.
#
# Devolve 0 se mexeu no arquivo, 1 se ele já estava como devia.
escrever_na_lista_serpro() { # <caminho-da-biblioteca>
    local valor=$1 tmp
    if [ -e "$LISTA_SERPRO" ] && grep -qxF "$CHAVE_LISTA=$valor" "$LISTA_SERPRO"; then
        return 1
    fi
    mkdir -p "$(dirname "$LISTA_SERPRO")"
    tmp=$(mktemp "$LISTA_SERPRO.XXXXXX")
    if [ -e "$LISTA_SERPRO" ]; then
        # O grep devolve 1 quando não sobra linha nenhuma, que é o caso de um
        # arquivo que só tinha a nossa. Com 'set -e' ligado, isso derrubaria o
        # script aqui em vez de escrever o arquivo.
        { grep -v "^$CHAVE_LISTA=" "$LISTA_SERPRO" || true; } > "$tmp"
    fi
    printf '%s=%s\n' "$CHAVE_LISTA" "$valor" >> "$tmp"
    mv "$tmp" "$LISTA_SERPRO"
}

# Devolve 0 se removeu, 1 se não havia o que remover.
remover_da_lista_serpro() {
    local tmp
    [ -e "$LISTA_SERPRO" ] || return 1
    grep -q "^$CHAVE_LISTA=" "$LISTA_SERPRO" || return 1
    tmp=$(mktemp "$LISTA_SERPRO.XXXXXX")
    { grep -v "^$CHAVE_LISTA=" "$LISTA_SERPRO" || true; } > "$tmp"
    mv "$tmp" "$LISTA_SERPRO"
    # Um arquivo que ficou vazio era só nosso: quem desinstalou não tem por que
    # continuar com um arquivo de configuração de outro programa, escrito por
    # nós, sem nada dentro. O rmdir só apaga o diretório se ele também tiver
    # ficado vazio, o que quer dizer que o Assinador Serpro não está ali.
    [ -s "$LISTA_SERPRO" ] || rm -f "$LISTA_SERPRO"
    rmdir "$(dirname "$LISTA_SERPRO")" 2>/dev/null || true
    return 0
}

# O Assinador Serpro está nesta máquina?
#
# Escrever a lista sem ele instalado seria inventar programa, do mesmo jeito
# que criar ~/.config/vivaldi numa máquina sem Vivaldi. Mas o pacote do Serpro
# não é Flatpak, não está no Flathub e não deixa nome de comando que se possa
# assumir, então se pergunta pelos dois rastros que ele deixa: o diretório de
# configuração, que ele cria ao ser aberto, e o atalho de menu, que o pacote
# instala. Um dos dois basta.
assinador_serpro_presente() {
    local dir
    [ -d "$(dirname "$LISTA_SERPRO")" ] && return 0
    for dir in "${XDG_DATA_HOME:-$HOME/.local/share}/applications" \
               /usr/local/share/applications /usr/share/applications; do
        [ -d "$dir" ] || continue
        # O padrão é apertado de propósito: casa o "Assinador Serpro" do
        # atalho dele e não casa o "assinatura;serpro" das Keywords do atalho
        # do SerproID, que é outro programa e é nosso.
        grep -rlis 'assinador[ ._-]*serpro' "$dir" >/dev/null 2>&1 && return 0
    done
    return 1
}

# Atalhos de menu vindos das extensões. O .desktop leva o prefixo do
# aplicativo porque é assim que se reconhece, mais tarde, o que este projeto
# escreveu; o ícone vai para um diretório próprio e é referenciado por caminho
# absoluto, que é o que o próprio fabricante faz e o que dispensa adivinhar o
# tamanho declarado num tema de ícones.
ATALHOS_HOST="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
ICONES_HOST="${XDG_DATA_HOME:-$HOME/.local/share}/$APP_ID"

# A série do p11-kit do host, no formato "0.26". Sai do próprio p11-kit, e não
# do nome do arquivo: o soname distingue as séries por acaso e ordena errado
# como texto. Ver src/adv-br-serie.sh para o que depende disso.
serie_p11kit_host() {
    p11-kit list-modules 2>/dev/null |
        sed -n '/^module: p11-kit-trust/,/^module:/p' |
        sed -n 's/^ *library-version: *//p' | head -1
}

# Registro de módulo no banco NSS, sem nss-tools.
#
# O 'modutil' seria o caminho óbvio, e não é usado de propósito: ele não vem
# instalado em toda distribuição (num Fedora atômico, acrescentá-lo custa um
# rpm-ostree e um reboot), e o que ele faz num banco moderno é editar um
# arquivo de texto: pkcs11.txt, ao lado do cert9.db. Ver host/nssdb.py.
nss() { python3 "$AQUI/nssdb.py" "$@"; }
