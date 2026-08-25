# shellcheck shell=sh
# Manda o stderr deste processo para o arquivo de log do módulo.
#
# Não é um programa: é para ser incluído com ".", ANTES de qualquer outra
# coisa, por quem define ADV_BR_MODULO. Quem inclui isto depois de já ter
# escrito alguma mensagem perde justamente as mensagens que interessam.
#
# Por que o stderr, e por que só ele: três processos deste aplicativo têm o
# stdout reservado para protocolo binário (a ponte PKCS#11 fala RPC do p11-kit,
# os assinadores falam native messaging), e um byte a mais ali corrompe a
# conversa. O stderr, por outro lado, hoje se perde: quem inicia esses
# processos é o navegador ou o p11-kit do sistema, e ninguém guarda o que eles
# escrevem. É exatamente o que falta quando alguém diz "não funcionou aqui".
#
# Nada aqui pode falhar de forma a impedir o processo de rodar. Sem lugar para
# escrever, o log some e o aplicativo continua: um token que assina vale mais
# que um registro do motivo de ele não assinar.

ADV_BR_LOGS="${XDG_DATA_HOME:-$HOME/.local/share}/logs"

# O nome do módulo do assinador vem de um argumento que quem chama é o
# navegador, e vira nome de arquivo aqui. Sem esta limpeza, um argumento com
# barras escreveria fora do diretório de logs.
ADV_BR_MODULO=$(printf '%s' "${ADV_BR_MODULO:-desconhecido}" |
    tr -c 'a-zA-Z0-9-' '_' | cut -c1-64)

if mkdir -p "$ADV_BR_LOGS" 2>/dev/null; then
    ADV_BR_LOG="$ADV_BR_LOGS/${ADV_BR_MODULO:-desconhecido}.log"

    # Rotação por tamanho, com um arquivo velho só. É a ponte que mais escreve,
    # e ela roda a cada abertura de navegador: sem isto, meses depois o log é
    # um arquivo de dezenas de MB que ninguém consegue mandar junto de um
    # relato de erro.
    if [ -f "$ADV_BR_LOG" ]; then
        ADV_BR_TAM=$(wc -c < "$ADV_BR_LOG" 2>/dev/null || echo 0)
        if [ "${ADV_BR_TAM:-0}" -gt 1048576 ]; then
            mv -f "$ADV_BR_LOG" "$ADV_BR_LOG.1" 2>/dev/null || true
        fi
        unset ADV_BR_TAM
    fi

    if exec 2>>"$ADV_BR_LOG"; then
        # A marca de início é o que separa uma execução da seguinte quando
        # alguém abre o arquivo três semanas depois.
        printf '\n=== %s · %s ===\n' "$(date '+%Y-%m-%d %H:%M:%S')" \
            "${ADV_BR_MODULO:-desconhecido}" >&2
    fi
fi

export ADV_BR_LOGS
