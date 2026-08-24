#!/usr/bin/env bash
# Publica, para os navegadores do host, o que mora dentro do Flatpak adv-br.
#
# São dois problemas diferentes, e é por isso que são dois adaptadores:
#
#   autenticação por certificado   o navegador carrega o módulo PKCS#11 dentro
#   (Projudi, eproc, login do PJe, do próprio processo. Escrevemos um .module
#   gov.br)                        que manda o p11-kit iniciar um 'flatpak run'
#                                  e conversar com ele pelo pipe.
#
#   assinatura pelos assinadores   quem fala com o token é um programa à parte,
#   (SAJ, portal da OAB, Lacuna)   que o navegador executa e com quem conversa
#                                  por stdin/stdout. Escrevemos o manifesto de
#                                  native messaging apontando para um atalho
#                                  que entra no Flatpak.
#
# Nada é instalado no host: o que se escreve são arquivos de configuração
# dentro da sua própria home.
set -euo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/comum.sh"

ACAO=publicar
CONCEDER=0

ajuda() {
    /usr/bin/cat <<'FIM'
Uso: ./host/publicar.sh [opções]

  --conceder   executa as permissões de Flatpak que este script imprimiria,
               em vez de só mostrá-las
  --listar     mostra o que está publicado hoje
  --remover    desfaz tudo o que este script publicou
  --ajuda      esta mensagem

Sobre o --conceder: navegador em Flatpak precisa de permissões que afrouxam o
confinamento do aplicativo. São decisão de quem usa a máquina, então por padrão
este script imprime os comandos e não os executa.

Idempotente: pode ser executado quantas vezes quiser.
FIM
}

while [ $# -gt 0 ]; do
    case $1 in
        --conceder) CONCEDER=1 ;;
        --listar)   ACAO=listar ;;
        --remover)  ACAO=remover ;;
        --ajuda|-h) ajuda; exit 0 ;;
        *)          erro "opção desconhecida: $1 (use --ajuda)" ;;
    esac
    shift
done

if [ "$CONCEDER" -eq 1 ] && [ "$ACAO" != publicar ]; then
    erro "--conceder concede permissões ao publicar; não combina com --$ACAO.
      Para tirá-las, ./host/publicar.sh --remover imprime os comandos."
fi

command -v flatpak >/dev/null || erro "flatpak não encontrado no PATH."
command -v python3 >/dev/null || erro "python3 não encontrado no PATH."

CONFIG_HOST="${XDG_CONFIG_HOME:-$HOME/.config}"

# Percorre a tabela de navegadores nas duas encarnações possíveis (host e
# Flatpak) e chama a função dada com:
#
#     <id-ou-vazio> <familia> <bancos-nss> <dir-manifestos>
#
# id vazio quer dizer "este é o navegador do host". Foi extraído em função
# porque publicar, listar e remover percorrem exatamente a mesma coisa, e
# esquecer um dos lados num deles deixaria lixo impossível de encontrar.
para_cada_navegador() {
    local funcao=$1 navegador id familia perfis manifestos
    for navegador in "${NAVEGADORES[@]}"; do
        IFS='|' read -r id familia perfis manifestos <<<"$navegador"

        "$funcao" "" "$familia" \
            "$(expandir "$perfis" "$HOME" "$CONFIG_HOST")" \
            "$(expandir "$manifestos" "$HOME" "$CONFIG_HOST")"

        [ -d "$HOME/.var/app/$id" ] || continue
        "$funcao" "$id" "$familia" \
            "$(expandir "$perfis" "$HOME/.var/app/$id" "$HOME/.var/app/$id/config")" \
            "$(expandir "$manifestos" "$HOME/.var/app/$id" "$HOME/.var/app/$id/config")"
    done
}

modulos_publicados() {
    [ -d "$MODULOS_HOST" ] || return 0
    find "$MODULOS_HOST" -maxdepth 1 -name "$PREFIXO_MODULO*.module" -printf '%f\n' \
        2>/dev/null | sort
}

# ---------------------------------------------------------------------------
if [ "$ACAO" = listar ]; then
    titulo "Módulos PKCS#11 publicados (autenticação)"
    achou=0
    while read -r modulo; do
        [ -n "$modulo" ] || continue
        achou=1
        ok "${modulo%.module} → $(sed -n "s/^remote: .*$APP_ID //p" "$MODULOS_HOST/$modulo")"
    done < <(modulos_publicados)
    [ "$achou" = 1 ] || log "nenhum."

    titulo "Bancos NSS com o módulo registrado"
    # A família Chromium inteira compartilha ~/.pki/nssdb; sem isto o mesmo
    # banco apareceria uma vez por navegador da tabela.
    declare -A JA_LISTADO=()
    listar_nss() {
        local id=$1 familia=$2 perfis=$3
        local rotulo=${id:-host} registrados
        while read -r banco; do
            [ -n "$banco" ] || continue
            [ -n "${JA_LISTADO[$banco]:-}" ] && continue
            JA_LISTADO[$banco]=1
            registrados=$(nss listar "$banco" |
                grep -E "^($NOME_NSS_HOST|$NOME_NSS_SANDBOX)	" | cut -f1 |
                tr '\n' ' ' || true)
            [ -n "$registrados" ] && ok "[$rotulo] $banco (${registrados% })"
        done < <(bancos_nss "$perfis")
        return 0
    }
    para_cada_navegador listar_nss

    titulo "Assinadores publicados (assinatura)"
    listar_manifestos() {
        local id=$1 familia=$2 perfis=$3 manifestos=$4
        local dir arquivo
        while read -r dir; do
            [ -d "$dir" ] || continue
            for arquivo in "$dir"/*.json; do
                [ -e "$arquivo" ] || continue
                grep -q "$PREFIXO_WRAPPER" "$arquivo" && ok "$arquivo"
            done
        done <<<"$manifestos"
        # 'set -e' está ligado, e um grep que não casou é o caso comum aqui
        # (todo navegador tem manifesto de outro programa). Sem este return, a
        # função devolveria o status dele e o laço de fora morreria calado.
        return 0
    }
    para_cada_navegador listar_manifestos

    titulo "Atalhos de menu"
    achou=0
    for arquivo in "$ATALHOS_HOST/$APP_ID."*.desktop; do
        [ -e "$arquivo" ] || continue
        achou=1
        ok "$arquivo"
    done
    [ "$achou" = 1 ] || log "nenhum."
    printf '\n'
    exit 0
fi

# ---------------------------------------------------------------------------
if [ "$ACAO" = remover ]; then
    titulo "Removendo o que este projeto publicou"

    while read -r modulo; do
        [ -n "$modulo" ] || continue
        rm -f "$MODULOS_HOST/$modulo" && ok "módulo $modulo"
    done < <(modulos_publicados)

    remover_de() {
        local id=$1 familia=$2 perfis=$3 manifestos=$4
        local banco dir arquivo
        while read -r banco; do
            [ -n "$banco" ] || continue
            for nome in "$NOME_NSS_HOST" "$NOME_NSS_SANDBOX"; do
                if printf '%s\n' "$(nss listar "$banco")" | grep -q "^$nome	"; then
                    nss remover "$banco" "$nome" && ok "$nome removido de $banco"
                fi
            done
        done < <(bancos_nss "$perfis")

        while read -r dir; do
            [ -d "$dir" ] || continue
            for arquivo in "$dir"/*.json; do
                [ -e "$arquivo" ] || continue
                grep -q "$PREFIXO_WRAPPER" "$arquivo" &&
                    rm -f "$arquivo" && ok "manifesto $arquivo"
            done
        done <<<"$manifestos"
        return 0
    }
    para_cada_navegador remover_de

    rm -f "$BIN_HOST/$PREFIXO_WRAPPER"* 2>/dev/null || true
    find "$HOME/.var/app" -maxdepth 5 \
        \( -path "*/$SUBDIR_ATALHO_FLATPAK/$PREFIXO_WRAPPER*" \
           -o -path "*/.local/bin/$PREFIXO_WRAPPER*" \) \
        -delete 2>/dev/null || true
    # E o diretório que os continha, que é nosso: apagar só os arquivos deixa
    # um ~/.var/app/<navegador>/data/adv-br vazio em cada navegador, que quem
    # desinstalou não tem por que encontrar depois.
    find "$HOME/.var/app" -maxdepth 3 -type d -name "$(basename "$SUBDIR_ATALHO_FLATPAK")" \
        -empty -delete 2>/dev/null || true
    ok "atalhos de assinador removidos"

    for arquivo in "$ATALHOS_HOST/$APP_ID."*.desktop; do
        [ -e "$arquivo" ] || continue
        rm -f "$arquivo" && ok "atalho de menu $(basename "$arquivo")"
    done
    rm -rf "$ICONES_HOST"
    command -v update-desktop-database >/dev/null &&
        update-desktop-database "$ATALHOS_HOST" 2>/dev/null || true

    # As permissões do Flatpak não são revogadas junto: quem não as concedeu
    # não as tira. O --conceder é opt-in, e desfazê-lo também.
    printf '\n  As permissões de Flatpak continuam como estão. Para tirá-las:\n'
    printf '      systemctl --user disable --now p11-kit-server.socket\n'
    printf '      flatpak override --user --nofilesystem=xdg-run/p11-kit/pkcs11 <navegador>\n'
    printf '      flatpak override --user --no-talk-name=org.freedesktop.Flatpak <navegador>\n'
    printf '\n  Feche e reabra os navegadores.\n\n'
    exit 0
fi

# ---------------------------------------------------------------------------
flatpak_instalado "$APP_ID" ||
    erro "$APP_ID não está instalado. Rode antes: ./instalar.sh"

PROXY=$(proxy_do_host) ||
    erro "p11-kit-proxy.so não encontrado no host. Instale o p11-kit:
          Fedora: sudo dnf install p11-kit p11-kit-server
          Debian: sudo apt install p11-kit
          Arch:   sudo pacman -S p11-kit"

# ---------------------------------------------------------------------------
titulo "1/4 · Drivers do token (autenticação por certificado)"

mkdir -p "$MODULOS_HOST"

# A lista sai do próprio pacote: quem sabe quais extensões de driver estão
# instaladas é ele. Um driver a mais é um .module a mais, sem editar script.
MODULOS=$(flatpak run --command=adv-br-modulos "$APP_ID" 2>/dev/null) ||
    erro "não consegui listar os módulos do $APP_ID."
[ -n "$MODULOS" ] || erro "o $APP_ID não tem módulo PKCS#11 nenhum, nem o OpenSC."

declare -A VIVOS=()
while IFS=$'\t' read -r rotulo biblioteca; do
    [ -n "$rotulo" ] || continue
    VIVOS[$PREFIXO_MODULO$rotulo.module]=1
    {
        printf '# Escrito por %s (host/publicar.sh).\n' "$APP_ID"
        printf '# O p11-kit inicia este comando sob demanda e conversa com ele pelo\n'
        printf '# pipe; do outro lado está o driver, dentro do Flatpak.\n'
        printf 'remote: |flatpak run --command=adv-br-pkcs11 %s %s\n' "$APP_ID" "$biblioteca"
    } > "$MODULOS_HOST/$PREFIXO_MODULO$rotulo.module"
    ok "$rotulo"
done <<<"$MODULOS"

# Um .module de driver que já não existe lá dentro faria o p11-kit tentar
# abri-lo em toda inicialização de navegador, e falhar.
while read -r modulo; do
    [ -n "$modulo" ] || continue
    [ -n "${VIVOS[$modulo]:-}" ] && continue
    rm -f "$MODULOS_HOST/$modulo" && log "$modulo não existe mais no pacote; removido."
done < <(modulos_publicados)

# ---------------------------------------------------------------------------
titulo "2/4 · Bancos NSS"

# Dois registros diferentes, porque são dois problemas:
#
#   navegador do host      carrega o p11-kit-proxy do host, que já lê os
#                          .module escritos acima.
#   navegador em Flatpak   não lê módulo de usuário nenhum: todo sandbox recebe
#                          um /etc/pkcs11/pkcs11.conf com "user-config: none".
#                          A porta que sobra é o p11-kit-client.so do runtime
#                          dele, que fala com o socket do p11-kit do host, e
#                          esse socket serve tudo o que o host conhece,
#                          inclusive os módulos acima.
FLATPAKS_COM_CLIENT=()
FLATPAKS_COM_ASSINADOR=()
# Toda a família Chromium compartilha um banco só, ~/.pki/nssdb. Sem isto, o
# mesmo registro seria feito (e anunciado) uma vez por navegador da tabela.
declare -A BANCO_VISTO=()

# Um banco que vive no home real é lido dos dois lados: pelo programa do host,
# que carrega o p11-kit-proxy, e por um Flatpak com --filesystem=home, que só
# tem o p11-kit-client. Por isso ele recebe os dois registros. O NSS ignora em
# silêncio o módulo que não conseguir carregar, e é justamente um deles que
# nunca carrega em cada lado.
registrar_banco() {
    local banco=$1 onde=$2
    [ -n "${BANCO_VISTO[$banco]:-}" ] && return 0
    BANCO_VISTO[$banco]=1

    case $banco in
        "$HOME"/.var/app/*)
            # Banco privado de um Flatpak: só o lado de dentro o abre.
            nss registrar "$banco" "$NOME_NSS_SANDBOX" "$CLIENT_NO_SANDBOX" &&
                ok "$onde: $banco" ||
                aviso "$onde: não consegui registrar em $banco"
            ;;
        *)
            nss registrar "$banco" "$NOME_NSS_HOST" "$PROXY" &&
                nss registrar "$banco" "$NOME_NSS_SANDBOX" "$CLIENT_NO_SANDBOX" &&
                ok "$onde: $banco" ||
                aviso "$onde: não consegui registrar em $banco"
            ;;
    esac
}

registrar_nss() {
    local id=$1 familia=$2 perfis=$3
    local banco alcancou=0
    while read -r banco; do
        [ -n "$banco" ] || continue
        alcancou=1
        registrar_banco "$banco" "${id:-host}"
    done < <(bancos_nss "$perfis")
    [ "$alcancou" = 1 ] && [ -n "$id" ] && FLATPAKS_COM_CLIENT+=("$id")
    return 0
}
para_cada_navegador registrar_nss

# Aplicativos que não são navegador e leem o banco NSS do home real: o registro
# já foi feito acima, junto com o do host. O que falta para eles é a permissão
# de alcançar o socket do p11-kit.
for consumidor in "${CONSUMIDORES[@]}"; do
    IFS='|' read -r id para_que <<<"$consumidor"
    flatpak_instalado "$id" || continue
    log "$id ($para_que): usa o banco NSS do seu home, já registrado acima."
    FLATPAKS_COM_CLIENT+=("$id")
done

# ---------------------------------------------------------------------------
titulo "3/4 · Assinadores (assinatura em navegador)"

ASSINADORES=$(flatpak run --command=adv-br-assinadores "$APP_ID" 2>/dev/null) || true

# O que sobreviver a esta publicação. O que não estiver aqui é rastro de um
# assinador removido: um manifesto apontando para um atalho que já não existe
# faz a extensão do navegador dizer que o assinador não está instalado, que é
# o mesmo sintoma de nunca ter sido publicado.
declare -A MANIFESTO_VIVO=()
declare -A WRAPPER_VIVO=()
while IFS=$'\t' read -r nome _ comando _; do
    [ -n "$nome" ] || continue
    MANIFESTO_VIVO["$nome.json"]=1
    WRAPPER_VIVO["$PREFIXO_WRAPPER${comando#adv-br-}"]=1
done <<<"$ASSINADORES"

# Roda mesmo quando não há assinador nenhum instalado: é justamente aí que há
# mais o que limpar.
limpar_assinadores_removidos() {
    local id=$1 familia=$2 perfis=$3 manifestos=$4
    local dir arquivo
    while read -r dir; do
        [ -d "$dir" ] || continue
        for arquivo in "$dir"/*.json; do
            [ -e "$arquivo" ] || continue
            grep -q "$PREFIXO_WRAPPER" "$arquivo" || continue
            [ -n "${MANIFESTO_VIVO[$(basename "$arquivo")]:-}" ] && continue
            rm -f "$arquivo" && log "$(basename "$arquivo") não existe mais no pacote; removido."
        done
    done <<<"$manifestos"
    return 0
}
para_cada_navegador limpar_assinadores_removidos

for arquivo in "$BIN_HOST/$PREFIXO_WRAPPER"* \
               "$HOME"/.var/app/*/"$SUBDIR_ATALHO_FLATPAK"/"$PREFIXO_WRAPPER"*; do
    [ -e "$arquivo" ] || continue
    [ -n "${WRAPPER_VIVO[$(basename "$arquivo")]:-}" ] && continue
    rm -f "$arquivo" && log "atalho $(basename "$arquivo") removido."
done

if [ -z "$ASSINADORES" ]; then
    log "nenhum assinador instalado.
      Para assinar em navegador:  ./instalar.sh --with-webpki"
else
    mkdir -p "$BIN_HOST"

    # Um wrapper por assinador no host, e um por assinador dentro do home de
    # cada navegador em Flatpak. São arquivos diferentes porque o comando é
    # diferente: de dentro de um sandbox não existe 'flatpak', só o portal, que
    # se alcança com flatpak-spawn --host.
    escrever_manifesto() { # <json> <path> <destino>
        python3 -c '
import json, sys
manifesto = json.loads(sys.argv[1])
manifesto["path"] = sys.argv[2]
with open(sys.argv[3], "w", encoding="utf-8") as f:
    json.dump(manifesto, f, ensure_ascii=False, indent=2)
    f.write("\n")
' "$1" "$2" "$3"
    }

    # familia_do_manifesto, e não familia: 'para_cada_navegador' declara uma
    # variável local com esse nome, para a família do navegador da vez, e ela
    # sombreia a de fora. O efeito era mudo e caro: a comparação lá dentro
    # virava "a família do navegador é igual a ela mesma", sempre verdadeira, e
    # cada navegador recebia o manifesto do último assinador processado: o do
    # Firefox, com allowed_extensions, dentro do diretório do Chrome, que exige
    # allowed_origins. O navegador ignora o arquivo sem dizer nada e a extensão
    # informa que o assinador não está instalado.
    while IFS=$'\t' read -r nome familia_do_manifesto comando manifesto; do
        [ -n "$nome" ] || continue
        curto=${comando#adv-br-}

        wrapper_host="$BIN_HOST/$PREFIXO_WRAPPER$curto"
        {
            printf '#!/bin/sh\n'
            printf '# Escrito por %s (host/publicar.sh).\n' "$APP_ID"
            printf 'exec flatpak run --command=%s %s "$@"\n' "$comando" "$APP_ID"
        } > "$wrapper_host"
        chmod +x "$wrapper_host"

        publicar_assinador() {
            local id=$1 fam=$2 perfis=$3 manifestos=$4
            [ "$fam" = "$familia_do_manifesto" ] || return 0
            local dir raiz wrapper_flatpak
            while read -r dir; do
                # O diretório-pai é o que diz se este navegador existe aqui:
                # criar ~/.config/vivaldi numa máquina sem Vivaldi seria
                # inventar navegador.
                [ -d "$(dirname "$dir")" ] || continue
                mkdir -p "$dir"
                if [ -z "$id" ]; then
                    escrever_manifesto "$manifesto" "$wrapper_host" "$dir/$nome.json"
                else
                    raiz="$HOME/.var/app/$id"
                    mkdir -p "$raiz/$SUBDIR_ATALHO_FLATPAK"
                    wrapper_flatpak="$raiz/$SUBDIR_ATALHO_FLATPAK/$PREFIXO_WRAPPER$curto"
                    {
                        printf '#!/bin/sh\n'
                        printf '# Escrito por %s (host/publicar.sh).\n' "$APP_ID"
                        printf '# Roda DENTRO do sandbox do navegador: aqui não existe\n'
                        printf '# flatpak, só o portal, alcançado por flatpak-spawn --host.\n'
                        printf 'exec flatpak-spawn --host flatpak run --command=%s %s "$@"\n' \
                            "$comando" "$APP_ID"
                    } > "$wrapper_flatpak"
                    chmod +x "$wrapper_flatpak"
                    # O mesmo caminho absoluto vale dentro e fora do
                    # sandbox; ver SUBDIR_ATALHO_FLATPAK em host/comum.sh.
                    escrever_manifesto "$manifesto" \
                        "$wrapper_flatpak" "$dir/$nome.json"
                    FLATPAKS_COM_ASSINADOR+=("$id")
                fi
            done <<<"$manifestos"
            return 0
        }
        para_cada_navegador publicar_assinador
        ok "$nome ($familia_do_manifesto)"
    done <<<"$ASSINADORES"
fi

# ---------------------------------------------------------------------------
titulo "4/4 · Atalhos de aplicativo"

# Algumas extensões trazem aplicativo, não só biblioteca. O SerproID é o caso:
# sem abrir o aplicativo uma vez para associar o certificado, não há o que
# assinar. Um .desktop dentro de uma extensão não é exportado pelo Flatpak, que
# só exporta o do aplicativo, e no momento em que ele foi construído. Daí este
# passo.
ATALHOS=$(flatpak run --command=adv-br-atalhos "$APP_ID" 2>/dev/null) || true

if [ -z "$ATALHOS" ]; then
    log "nenhuma extensão instalada oferece atalho."
else
    mkdir -p "$ATALHOS_HOST" "$ICONES_HOST"
    while IFS=$'\t' read -r nome _ _; do
        [ -n "$nome" ] || continue
        icone="$ICONES_HOST/$nome.png"
        if flatpak run --command=adv-br-atalhos "$APP_ID" "$nome" icone > "$icone" 2>/dev/null &&
            [ -s "$icone" ]; then
            :
        else
            rm -f "$icone"
            icone=""
        fi
        flatpak run --command=adv-br-atalhos "$APP_ID" "$nome" desktop 2>/dev/null |
            sed -e "s|@EXEC@|flatpak run --command=adv-br-ferramentas $APP_ID $nome|" \
                -e "s|@ICONE@|$icone|" \
            > "$ATALHOS_HOST/$APP_ID.$nome.desktop"
        ok "$nome"
    done <<<"$ATALHOS"
    # Sem isto, alguns menus só veem o atalho novo no próximo login.
    command -v update-desktop-database >/dev/null &&
        update-desktop-database "$ATALHOS_HOST" 2>/dev/null || true
fi

# ---------------------------------------------------------------------------
# As permissões do Flatpak
#
# Impressas e não executadas por padrão: o socket do p11-kit expõe ao sandbox
# todos os módulos PKCS#11 do host, não só os deste projeto, e o --talk-name
# abre ao navegador a porta de executar coisas fora do sandbox.
# ---------------------------------------------------------------------------
unicos() { printf '%s\n' "$@" | grep -v '^$' | sort -u; }

COM_CLIENT=$(unicos "${FLATPAKS_COM_CLIENT[@]+"${FLATPAKS_COM_CLIENT[@]}"}")
COM_ASSINADOR=$(unicos "${FLATPAKS_COM_ASSINADOR[@]+"${FLATPAKS_COM_ASSINADOR[@]}"}")

if [ -n "$COM_CLIENT$COM_ASSINADOR" ]; then
    titulo "Navegadores em Flatpak"
    if [ "$CONCEDER" -eq 1 ]; then
        if [ -n "$COM_CLIENT" ]; then
            if systemctl --user enable --now p11-kit-server.socket 2>/dev/null; then
                ok "p11-kit-server.socket habilitado"
            else
                aviso "não consegui habilitar o p11-kit-server.socket, e sem ele o
      sandbox não alcança módulo nenhum. Ele vem no pacote p11-kit-server
      (Fedora) ou p11-kit (Debian, Arch). Depois de instalar:
          systemctl --user enable --now p11-kit-server.socket"
            fi
            for app in $COM_CLIENT; do
                flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 "$app" &&
                    ok "$app: acesso ao socket do p11-kit (autenticação)"
            done
        fi
        for app in $COM_ASSINADOR; do
            flatpak override --user --talk-name=org.freedesktop.Flatpak "$app" &&
                ok "$app: acesso ao portal (assinatura)"
        done
    else
        printf '  Estes comandos são seus porque afrouxam o confinamento do navegador.\n'
        printf '  Para que este script os execute:  ./host/publicar.sh --conceder\n\n'
        [ -n "$COM_CLIENT" ] &&
            printf '      systemctl --user enable --now p11-kit-server.socket\n'
        for app in $COM_CLIENT; do
            printf '      flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 %s\n' "$app"
        done
        for app in $COM_ASSINADOR; do
            printf '      flatpak override --user --talk-name=org.freedesktop.Flatpak %s\n' "$app"
        done
    fi
fi

/usr/bin/cat <<'FIM'

  Feche os navegadores por inteiro e reabra: o módulo PKCS#11 e os manifestos
  são lidos na inicialização do processo.

  Conferir:
      ./host/testar-pkcs11.sh      o que o host enxerga do token
      ./host/testar-assinador.sh   conversa com o assinador como o navegador faria
      ./diagnostico.sh             o encanamento inteiro

  Para cada assinador, falta instalar a extensão no navegador que você usa:
      Lacuna Web PKI      https://get.webpkiplugin.com/
      Softplan WebSigner  https://websigner.softplan.com.br/
      Certisign WebSigner https://get.websignerplugin.com/

  E, na aba "Cripto Dispositivos" de cada extensão, em "Opções personalizadas",
  acrescentar o caminho:

      /pkcs11/adv-br.so

  Ele responde por todos os drivers instalados aqui, inclusive os que você
  instalar depois. As opções prontas da extensão apontam para /usr/lib, que
  dentro do sandbox pertence ao runtime e não pode receber driver nenhum.

  Desfazer:  ./host/publicar.sh --remover

FIM
