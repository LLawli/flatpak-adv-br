#!/usr/bin/env bash
# Constrói e instala o Flatpak adv-br, e o publica para os navegadores.
#
# Nada é instalado no host: o pacote é um Flatpak de usuário, e o que se
# escreve fora dele são arquivos de configuração dentro da sua própria home.
#
# Idempotente: rodar de novo reaproveita o que já está pronto.
set -euo pipefail

# RAIZ, e não AQUI: o host/comum.sh também define AQUI, para o diretório
# dele, e a segunda atribuição venceria.
RAIZ=$(cd "$(dirname "$0")" && pwd)
cd "$RAIZ"

. "$RAIZ/host/comum.sh"

MANIFESTO="$RAIZ/$APP_ID.yml"
REMOTO_LOCAL=adv-br-local

# Onde os artefatos de construção moram.
#
# NÃO no diretório do projeto: o flatpak-builder cria ali um .flatpak-builder
# que passa de 1 GB e um build-dir por extensão, e quem instalou por curl não
# tem por que descobrir isso depois. Em ~/.cache eles ficam onde se espera que
# fique cache: apagável a qualquer momento, e é o que --limpar faz.
#
# Também não em /tmp: numa máquina com /tmp em tmpfs, os 288 MB do SerproID
# seriam RAM.
# O subdiretório importa: --limpar apaga este caminho inteiro, e ele não pode
# ser um diretório que alguém possa estar usando para outra coisa.
TRABALHO=${ADV_BR_TRABALHO:-${XDG_CACHE_HOME:-$HOME/.cache}/flatpak-adv-br/construcao}
PEDIDOS=()
PUBLICAR=1
CONCEDER=0
REFAZER=0
LIMPAR=0

# A tabela do que é opcional mora em host/extensoes.sh, porque o
# desinstalar.sh precisa da mesma lista.
# Lido pelo host/extensoes.sh, que resolve os manifestos a partir daqui.
# shellcheck disable=SC2034
AQUI_RAIZ=$RAIZ
. "$RAIZ/host/extensoes.sh"

ajuda() {
    /usr/bin/cat <<'FIM'
Uso: ./instalar.sh [opções]

Sem nenhuma opção, instala só o pacote base: o OpenSC, que já reconhece parte
dos cartões ICP-Brasil, e a ponte que leva o token aos navegadores e ao Papers.
São poucos megabytes.

O resto se instala quando você for usar, e pode ser depois: rodar de novo com
outra opção acrescenta sem refazer o que já está pronto:

  drivers de token
    --with-safesign    token GD Burti, o mais usado na advocacia
    --with-safenet     eToken 5100, 5110, IDPrime
    --with-serproid    certificado em nuvem do Serpro (traz o aplicativo)
    --with-drivers     os três acima

  assinadores em navegador
    --with-webpki      Lacuna Web PKI
    --with-websigner   Softplan WebSigner, dos sistemas SAJ
    --with-certisign   Certisign WebSigner, do portal da OAB
    --with-assinadores os três acima

  aplicativos
    --with-pjeoffice   PJeOffice Pro, para assinar no PJe (CNJ)

  --with-tudo        tudo o que está acima
  --refazer          reconstrói o que já está instalado
  --limpar           apaga os artefatos de construção ao terminar
  --sem-publicar     só constrói e instala; não toca em nada fora do Flatpak
  --conceder         concede as permissões dos navegadores em Flatpak
  --ajuda            esta mensagem

Nada disso vem no pacote base porque nada disso pode ser redistribuído: cada
extensão baixa da URL do próprio fabricante, na sua máquina. E porque quem não
usa o PJe não deve baixar 300 MB de Java para descobrir isso.

O comando é idempotente: pode ser repetido à vontade.
FIM
}

while [ $# -gt 0 ]; do
    case $1 in
        --with-drivers)     PEDIDOS+=("${DRIVERS_TODOS[@]}") ;;
        --with-assinadores) PEDIDOS+=("${ASSINADORES_TODOS[@]}") ;;
        --with-tudo)        PEDIDOS+=("${!EXTENSOES[@]}") ;;
        --with-*)
            alvo=${1#--with-}
            [ -n "${EXTENSOES[$alvo]:-}" ] ||
                erro "não conheço '--with-$alvo'. Veja ./instalar.sh --ajuda"
            PEDIDOS+=("$alvo")
            ;;
        --refazer)       REFAZER=1 ;;
        --limpar)        LIMPAR=1 ;;
        --sem-publicar)  PUBLICAR=0 ;;
        --conceder)      CONCEDER=1 ;;
        --ajuda|-h)      ajuda; exit 0 ;;
        *) erro "opção desconhecida: $1 (use --ajuda)" ;;
    esac
    shift
done

# ---------------------------------------------------------------------------
titulo "Requisitos"

command -v flatpak >/dev/null || erro "flatpak não encontrado. Instale-o:
      Fedora: sudo dnf install flatpak
      Debian: sudo apt install flatpak
      Arch:   sudo pacman -S flatpak"
ok "flatpak $(flatpak --version | awk '{print $2}')"

if ! command -v flatpak-builder >/dev/null; then
    if flatpak info org.flatpak.Builder >/dev/null 2>&1; then
        construtor=(flatpak run org.flatpak.Builder)
        ok "flatpak-builder (Flatpak org.flatpak.Builder)"
    else
        erro "flatpak-builder não encontrado. Instale-o:
      Fedora: sudo dnf install flatpak-builder
      Debian: sudo apt install flatpak-builder
      Arch:   sudo pacman -S flatpak-builder
      ou, sem tocar no sistema:
              flatpak install --user flathub org.flatpak.Builder"
    fi
else
    construtor=(flatpak-builder)
    ok "flatpak-builder $(flatpak-builder --version | sed 's/.*-//')"
fi

command -v python3 >/dev/null || erro "python3 não encontrado; ele registra os
      módulos nos bancos NSS e monta os manifestos dos assinadores."
ok "python3 $(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])')"

if proxy_do_host >/dev/null; then
    ok "p11-kit ($(proxy_do_host))"
else
    aviso "p11-kit-proxy.so não encontrado. Sem ele, o navegador do host não
      alcança o token. Instale:
          Fedora: sudo dnf install p11-kit p11-kit-server
          Debian: sudo apt install p11-kit
          Arch:   sudo pacman -S p11-kit"
fi

# O daemon do PC/SC roda no HOST, não no sandbox: é ele quem fala com a
# leitora. Dentro do Flatpak existe só a biblioteca cliente.
if [ -S /run/pcscd/pcscd.comm ] || systemctl is-active --quiet pcscd.socket 2>/dev/null ||
   systemctl is-active --quiet pcscd.service 2>/dev/null; then
    ok "pcscd de pé"
else
    aviso "o pcscd do host não está de pé, e é ele quem fala com a leitora.
          Fedora: sudo dnf install pcsc-lite pcsc-lite-ccid
          Debian: sudo apt install pcscd libccid
          Arch:   sudo pacman -S pcsclite ccid
      e depois:
          sudo systemctl enable --now pcscd.socket"
fi

# ---------------------------------------------------------------------------
titulo "Runtime"

remoto_flathub() {
    # A saída é capturada antes de filtrar: "cmd | grep -q" faz o grep sair na
    # primeira ocorrência, o produtor levar SIGPIPE e, com pipefail, o pipeline
    # inteiro retornar 141: sucesso que vira falha conforme a POSIÇÃO da linha
    # que casou.
    printf '%s\n' "$(flatpak remotes --columns=name)" | grep -qx flathub && return 0
    log "acrescentando o remoto flathub (usuário)"
    flatpak remote-add --user --if-not-exists flathub \
        https://dl.flathub.org/repo/flathub.flatpakrepo
}

garantir_runtime() { # <ref>
    flatpak info --user "$1" >/dev/null 2>&1 && { ok "$1"; return 0; }
    flatpak info "$1" >/dev/null 2>&1 && { ok "$1 (do sistema)"; return 0; }
    remoto_flathub
    log "instalando $1 ..."
    flatpak install --user --noninteractive flathub "$1"
}

RUNTIME=$(sed -n 's/^runtime: *//p' "$MANIFESTO" | head -1)
VERSAO_RUNTIME=$(sed -n "s/^runtime-version: *'\\?\\([^']*\\)'\\?/\\1/p" "$MANIFESTO" | head -1)
SDK=$(sed -n 's/^sdk: *//p' "$MANIFESTO" | head -1)
garantir_runtime "$RUNTIME//$VERSAO_RUNTIME"
garantir_runtime "$SDK//$VERSAO_RUNTIME"

# ---------------------------------------------------------------------------
titulo "p11-kit"

# A ponte PKCS#11 deste projeto é o remoting do p11-kit: o host inicia um
# processo dentro do sandbox e conversa com ele por um pipe, e o que trafega
# ali é a tabela de funções PKCS#11 serializada. As duas pontas precisam
# concordar sobre ela. Quando não concordam, nada recusa a conexão: os slots
# enumeram, o PIN é aceito, e toda assinatura falha: inclusive a do login por
# certificado, que assina no handshake TLS.
#
# O runtime é fixo, então quem varia é o host: Debian trixie e Ubuntu 24.04
# trazem a série 0.25 contra a 0.26 do runtime. Quando divergem, compilamos
# aqui um p11-kit da série do host, isolado em /app/lib/p11kit-compat, e só o
# processo da ponte o usa. O resto do pacote continua com o do runtime.
serie_p11kit_runtime() {
    flatpak run --user --command=sh "$RUNTIME//$VERSAO_RUNTIME" -c '
        for c in /usr/lib/*/pkcs11/p11-kit-trust.so /usr/lib/pkcs11/p11-kit-trust.so; do
            [ -e "$c" ] || continue
            printf "module: %s\ncritical: no\n" "$c" > /etc/pkcs11/modules/zz-serie.module
            p11-kit list-modules 2>/dev/null |
                sed -n "/^module: zz-serie/,/^module:/p" |
                sed -n "s/^ *library-version: *//p" | head -1
            exit 0
        done' 2>/dev/null | tr -d '\r'
}

COMPAT="$RAIZ/packaging/p11kit-compat.yml"

escrever_compat_neutro() {
    /usr/bin/cat > "$COMPAT" <<'FIM'
# GERADO por ./instalar.sh: não edite à mão.
#
# O p11-kit do host está na mesma série do runtime, então não há nada a
# compilar. Ver packaging/p11kit-series.txt.
name: p11kit-compat
buildsystem: simple
build-commands:
  - 'true  # host e runtime na mesma série do p11-kit; nada a fazer'
FIM
}

escrever_compat_com() { # <serie> <versao> <sha256>
    /usr/bin/cat > "$COMPAT" <<FIM
# GERADO por ./instalar.sh: não edite à mão.
#
# O p11-kit do host está na série $1 e o do runtime não. Este módulo compila um
# p11-kit $2 dentro do pacote, isolado em /app/lib/p11kit-compat, para que a
# ponte PKCS#11 fale a mesma língua dos dois lados. Só o processo da ponte o
# usa; o resto do pacote continua com o p11-kit do runtime.
name: p11kit-compat
buildsystem: meson
build-options:
  prefix: /app/lib/p11kit-compat
config-opts:
  # sysconfdir aponta para /etc, e não para dentro do prefixo: é lá que o
  # lançador registra os módulos, e sem isto este p11-kit procuraria a
  # configuração em /app/lib/p11kit-compat/etc e não acharia nada.
  - -Dsysconfdir=/etc
  - -Dsystemd=disabled
  - -Dbash_completion=disabled
  - -Dgtk_doc=false
  - -Dman=false
  - -Dnls=false
sources:
  - type: archive
    url: https://github.com/p11-glue/p11-kit/releases/download/$2/p11-kit-$2.tar.xz
    sha256: $3
FIM
}

SERIE_HOST=$(serie_p11kit_host)
SERIE_RUNTIME=$(serie_p11kit_runtime)

if [ -z "$SERIE_HOST" ] || [ -z "$SERIE_RUNTIME" ]; then
    aviso "não consegui comparar as séries do p11-kit (host: '${SERIE_HOST:-?}',
      runtime: '${SERIE_RUNTIME:-?}'). Seguindo sem compatibilidade; se a
      autenticação por certificado falhar no navegador com o token aparecendo
      na lista, é provavelmente isto."
    escrever_compat_neutro
elif [ "$SERIE_HOST" = "$SERIE_RUNTIME" ]; then
    ok "host e runtime na série $SERIE_HOST"
    escrever_compat_neutro
else
    linha=$(grep -E "^${SERIE_HOST}[[:space:]]" "$RAIZ/packaging/p11kit-series.txt" 2>/dev/null | head -1)
    if [ -z "$linha" ]; then
        aviso "o p11-kit do host está na série $SERIE_HOST e o do runtime na
      $SERIE_RUNTIME, e não há versão conhecida para a $SERIE_HOST em
      packaging/p11kit-series.txt. A autenticação por certificado no navegador
      não vai funcionar: acrescente a versão lá, ou alinhe o p11-kit do host."
        escrever_compat_neutro
    else
        # shellcheck disable=SC2086
        set -- $linha
        log "host na série $SERIE_HOST, runtime na $SERIE_RUNTIME: compilando p11-kit $2"
        escrever_compat_com "$1" "$2" "$3"
        ok "p11-kit $2 será embutido para a ponte"
    fi
fi

# ---------------------------------------------------------------------------
titulo "Construindo o $APP_ID"

# --disable-rofiles-fuse: em sistema de arquivos sem suporte a rofiles-fuse
# (btrfs com composefs, e o overlay de contêiner) o build falha lá pelo meio,
# com um erro que não diz o que fazer.
mkdir -p "$TRABALHO"
if "${construtor[@]}" --user --force-clean --disable-rofiles-fuse \
    --state-dir "$TRABALHO/estado" --install "$TRABALHO/base" "$MANIFESTO"; then
    ok "$APP_ID instalado"
else
    # A construção pode ter terminado e só a instalação ter falhado: o
    # --install do flatpak-builder consulta o remoto de onde vieram as
    # dependências, e um Flathub lento derruba tudo no último passo, depois de
    # meia hora de compilação. Visto acontecendo, com a rede boa:
    #   "While fetching .../summaries/….gz: [28] Timeout was reached"
    #
    # O que já está construído não precisa de remoto nenhum para ser
    # instalado. Exportar para um repositório local e instalar de lá é o mesmo
    # resultado, sem rede.
    [ -d "$TRABALHO/base/files" ] ||
        erro "a construção falhou (não há $TRABALHO/base/files); veja o erro acima."

    aviso "a construção terminou, mas a instalação falhou: normalmente é o
      remoto lento no último passo. Instalando a partir do que já foi
      construído, sem consultar remoto."

    REPO_LOCAL="$TRABALHO/repo"
    flatpak build-export "$REPO_LOCAL" "$TRABALHO/base" master >/dev/null ||
        erro "não consegui exportar o build para $REPO_LOCAL."
    flatpak remote-add --user --if-not-exists --no-gpg-verify \
        "$REMOTO_LOCAL" "$REPO_LOCAL" >/dev/null 2>&1 || true
    flatpak install --user --noninteractive --reinstall --no-deps \
        "$REMOTO_LOCAL" "$APP_ID" >/dev/null ||
        erro "não consegui instalar a partir de $REPO_LOCAL."
    # O remoto era só o veículo: deixá-lo cadastrado faria o 'flatpak update'
    # procurar atualização num diretório que ninguém mantém.
    # --force: sem ele o remote-delete pergunta se deve remover as refs que
    # vieram de lá, que é justamente o que acabou de ser instalado.
    flatpak remote-delete --user --force "$REMOTO_LOCAL" >/dev/null 2>&1 || true
    ok "$APP_ID instalado (a partir do repositório local)"
fi

# ---------------------------------------------------------------------------
if [ ${#PEDIDOS[@]} -gt 0 ]; then
    titulo "Extensões"

    # Sem repetição: "--with-tudo --with-webpki" pede o Lacuna duas vezes, e
    # construir de novo o que acabou de ser construído é só tempo perdido.
    declare -A JA_PEDIDO=()
    for alvo in "${PEDIDOS[@]}"; do
        [ -n "${JA_PEDIDO[$alvo]:-}" ] && continue
        JA_PEDIDO[$alvo]=1

        manifesto="$RAIZ/${EXTENSOES[$alvo]}"
        [ -e "$manifesto" ] || { aviso "não há manifesto para $alvo."; continue; }

        # O id sai do próprio manifesto: manter uma segunda tabela de "opção →
        # id" só criaria um lugar a mais para desencontrar.
        extensao=$(id_da_extensao "$alvo")

        if [ "$REFAZER" = 0 ] && flatpak info --user "$extensao" >/dev/null 2>&1; then
            ok "$alvo já instalado (--refazer reconstrói)"
            continue
        fi

        log "construindo $alvo ..."
        # Um diretório de build por extensão: um compartilhado misturaria as
        # árvores de duas delas.
        if "${construtor[@]}" --user --force-clean --disable-rofiles-fuse \
            --state-dir "$TRABALHO/estado" --install "$TRABALHO/$alvo" "$manifesto"; then
            ok "$alvo instalado"
        else
            aviso "$alvo falhou. As outras extensões seguem; para tentar de novo:
      ./instalar.sh --with-$alvo --refazer"
        fi
    done
fi

# ---------------------------------------------------------------------------
if [ "$LIMPAR" = 1 ]; then
    titulo "Limpeza"
    rm -rf "$TRABALHO"
    ok "artefatos de construção removidos de $TRABALHO"
fi

# ---------------------------------------------------------------------------
if [ "$PUBLICAR" = 1 ]; then
    if [ "$CONCEDER" = 1 ]; then
        "$RAIZ/host/publicar.sh" --conceder
    else
        "$RAIZ/host/publicar.sh"
    fi
else
    /usr/bin/cat <<FIM

  Instalado, e nada foi publicado (--sem-publicar). Quando quiser que os
  navegadores enxerguem o token:

      ./host/publicar.sh

FIM
fi
