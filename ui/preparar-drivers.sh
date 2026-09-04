# shellcheck shell=sh
# Preparo que todo processo que vai carregar um driver precisa fazer antes.
#
# Não é um programa: é para ser incluído com ".", e por isso não tem shebang
# nem set. Quem inclui são a ponte PKCS#11, os assinadores e os aplicativos que
# chegam como extensão, e o que os três têm em comum é abrir bibliotecas que
# vieram prontas pela rede, com os defeitos que o fabricante lhes deu.
#
# NADA aqui pode escrever em stdout: dois dos três chamadores usam o stdout
# como protocolo, e um byte a mais o corrompe.

COMPONENTES="${XDG_DATA_HOME:-$HOME/.local/share}/componentes"

# Bibliotecas de apoio que um componente tenha trazido junto. As do próprio
# aplicativo (o gdbm antigo que o SafeSign quer, por exemplo) já estão no
# caminho que o Flatpak monta.
for lib in "$COMPONENTES"/*/lib; do
    [ -d "$lib" ] || continue
    LD_LIBRARY_PATH="$lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
done
export LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}"

# A libserproidp11.so do SerproID usa símbolos da libgcc_s e NÃO a declara em
# DT_NEEDED: quem a abre com dlopen falha com "undefined symbol", a menos que
# alguma outra coisa já a tenha trazido para o processo. Pior, quando isso
# acontece por acaso o driver funciona, e o mesmo pacote passa numa máquina e
# falha em outra.
#
# No modelo de extensões isso era corrigido no build, com patchelf. Aqui não há
# build: o driver chega pronto pela rede. Carregá-la antes resolve, e é
# inofensivo, porque a libgcc_s já é carregada por quase tudo.
export LD_PRELOAD="libgcc_s.so.1${LD_PRELOAD:+:$LD_PRELOAD}"

# O SerproID faz readdir em ~/.config/serproid/certificados e derruba com
# SIGSEGV quem o carregou se o diretório não existir. Como quem carrega é o
# assinador, o navegador ou o PJeOffice, um driver mal preparado impediria
# assinar com qualquer outro token.
#
# O caminho é $HOME/.config, literal, e NÃO $XDG_CONFIG_HOME: no Flatpak os
# dois são diferentes ($HOME/config contra $HOME/.config), e a biblioteca monta
# o dela com o HOME, sem conhecer XDG. Criar em XDG_CONFIG_HOME faz o
# diretório nascer ao lado do que ela procura, e o SIGSEGV volta com cara de
# driver que "às vezes" derruba o assinador.
#
# E não basta criar: $HOME/.config não está montado neste sandbox, é tmpfs, e
# some ao fim da execução. É ali que o aplicativo do SerproID grava o
# certificado que a pessoa associa, então um mkdir simples faria esse trabalho
# ser perdido toda vez, sem erro nenhum. Daí o link para os dados do
# aplicativo, que persistem.
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/serproid/certificados"
if [ "${XDG_CONFIG_HOME:-$HOME/.config}" != "$HOME/.config" ]; then
    mkdir -p "$HOME/.config"
    [ -e "$HOME/.config/serproid" ] ||
        ln -s "$XDG_CONFIG_HOME/serproid" "$HOME/.config/serproid"
fi

# A libeToken do SafeNet procura a configuração dela em /etc por caminho
# absoluto, sem forma de redirecionar. No sandbox /etc é um tmpfs recriado a
# cada execução, então os arquivos que vieram no componente são copiados agora
# e somem junto com ele.
for conf in "$COMPONENTES"/*/etc/eToken*.conf; do
    [ -e "$conf" ] || continue
    cp -f "$conf" "/etc/$(basename "$conf")" ||
        echo "adv-br: não deu para preparar $(basename "$conf"); o token SafeNet pode não abrir." >&2
done
# A biblioteca guarda cache de token aqui e falha ao gravar se o diretório não
# existir. /var é gravável no sandbox.
mkdir -p /var/tmp/eToken.cache

# /pkcs11/adv-br.so: o caminho que se digita na aba "Cripto Dispositivos" das
# extensões dos assinadores.
#
# As opções prontas dessas extensões apontam para /usr/lib, que aqui pertence ao
# runtime e é somente leitura: não há como fazer o caminho de fábrica existir. O
# que dá para oferecer é um caminho curto, estável e digitável, e que responde
# por todos os drivers registrados, inclusive os instalados depois.
#
# Aponta para o SHIM, e não direto para o p11-kit-proxy do runtime, porque há
# quem canonize o caminho antes de guardá-lo: o PJeOffice grava o resultado de
# toRealPath() em ~/.pjeoffice-pro/pjeoffice-pro.config. Com o proxy no fim do
# link, o que ficaria gravado é libp11-kit.so.0.4.10, e a primeira atualização de
# runtime que mude esse número tira o driver da pessoa sem dizer nada. O shim é
# arquivo regular: o caminho real dele é ele mesmo. Ver src/pkcs11-shim.c.
#
# A raiz do sandbox é tmpfs, então o link nasce a cada execução e some com ela.
# É de propósito: nada disto precisa sobreviver ao processo.
if [ -e /app/lib/pkcs11/pkcs11.so ]; then
    { mkdir -p /pkcs11 && ln -sfn /app/lib/pkcs11/pkcs11.so /pkcs11/adv-br.so; } 2>/dev/null ||
        echo "adv-br: não consegui criar /pkcs11/adv-br.so; a extensão do assinador não vai achar o driver." >&2
fi

# --- RemoteID: o socket que atravessa as instâncias do sandbox --------------
#
# O RemoteID é certificado em NUVEM, e o módulo PKCS#11 dele não fala com a
# Certisign: ele manda o digest para o aplicativo do RemoteID por um socket
# UNIX, e é o aplicativo que pede o PIN e o código do autenticador e devolve a
# assinatura vinda do HSM.
#
# Isso é um problema de FRONTEIRA, não de caminho. Quem abre o módulo é a ponte
# do navegador, o assinador do Lacuna, o do Softplan, o do Certisign ou o
# PJeOffice, e cada um desses roda numa instância própria deste Flatpak. O
# aplicativo do RemoteID roda em OUTRA. O padrão do RemoteID é
# $XDG_RUNTIME_DIR/remoteid.sock, e o $XDG_RUNTIME_DIR é privado de cada
# instância: cada lado criaria o seu e nenhum acharia o outro. O sintoma seria o
# pior tipo, o que só aparece na hora de assinar — o certificado listado, o PIN
# nem pedido, e um erro de dispositivo.
#
# O que atravessa é o diretório de DADOS. ~/.var/app/<id>/data tem o mesmo
# caminho absoluto dentro e fora do sandbox, é o mesmo arquivo em todas as
# instâncias, e é alcançável também por programa do host. É a mesma propriedade
# de que ui/publicador.py depende para os atalhos dos navegadores em Flatpak.
#
# O RemoteID já prevê este caso: REMOTEID_SOCKET vence qualquer padrão dele.
REMOTEID="${XDG_DATA_HOME:-$HOME/.local/share}/remoteid"
mkdir -p "$REMOTEID" 2>/dev/null ||
    echo "adv-br: não deu para criar $REMOTEID; o RemoteID pode não assinar." >&2

# Modo de teste: um ARQUIVO, e não uma variável de ambiente.
#
# O teste local do RemoteID sobe um servidor falso e liga tudo com TEST_URL.
# Aqui essa variável precisa chegar a processos que ninguém lança à mão: a ponte
# que o p11-kit do host inicia sob demanda, o assinador que o navegador executa,
# o PJeOffice aberto pelo menu. Nenhum deles herda o ambiente de um terminal.
#
# Então o interruptor é um arquivo nos dados do aplicativo, e este preparo, que
# todos eles incluem, é quem o transforma em variável. Escrever o arquivo liga o
# modo de teste para o aplicativo, para a linha de comando, para o módulo, para
# o Papers e para o navegador de uma vez — que é a promessa de "um interruptor
# só" do RemoteID, mantida do lado de cá da fronteira. Ver ui/adv-br-remoteid.
REMOTEID_TESTE=""
if [ -s "$REMOTEID/TEST_URL" ]; then
    REMOTEID_TESTE=$(head -1 "$REMOTEID/TEST_URL" | tr -d '[:space:]')
    # Este arquivo é gravável por quem tiver acesso aos dados do aplicativo, e o
    # que estiver nele vira variável de ambiente de TODO processo preparado
    # aqui. Só uma URL http(s) sem caractere de shell atravessa a conferência.
    case "$REMOTEID_TESTE" in
        http://*|https://*)
            case "$REMOTEID_TESTE" in
                *[!A-Za-z0-9:/._-]*) REMOTEID_TESTE="" ;;
            esac
            ;;
        *) REMOTEID_TESTE="" ;;
    esac
    [ -n "$REMOTEID_TESTE" ] ||
        echo "adv-br: $REMOTEID/TEST_URL não é uma URL aceitável; modo de teste ignorado." >&2
fi

if [ -n "$REMOTEID_TESTE" ]; then
    export TEST_URL="$REMOTEID_TESTE"
    # Socket separado do de produção, de propósito: um aplicativo aberto em modo
    # de teste não pode responder por um pedido de assinatura de verdade.
    export REMOTEID_SOCKET="$REMOTEID/teste/remoteid.sock"
    mkdir -p "$REMOTEID/teste" 2>/dev/null || true

    # Com TEST_URL, o RemoteID reloca o estado para /tmp/remoteid-teste sem
    # consultar REMOTEID_HOME — é decisão dele, e é o que faz o teste ser um
    # interruptor só. Só que /tmp, no sandbox, é tmpfs de cada instância: o
    # estado que a linha de comando gravasse ali não chegaria ao aplicativo nem
    # ao módulo. A raiz do sandbox é gravável, então o caminho que ele espera
    # vira um link para os dados do aplicativo, e some com a execução.
    if [ -d /tmp/remoteid-teste ] && [ ! -L /tmp/remoteid-teste ]; then
        echo "adv-br: /tmp/remoteid-teste já existe como diretório; o modo de teste vai gravar num lugar que não persiste." >&2
    else
        ln -sfn "$REMOTEID/teste" /tmp/remoteid-teste 2>/dev/null ||
            echo "adv-br: não consegui ligar /tmp/remoteid-teste; o modo de teste não vai enxergar o estado." >&2
    fi
else
    export REMOTEID_SOCKET="$REMOTEID/remoteid.sock"
    # Junto do socket, e não no XDG_STATE_HOME que o RemoteID usaria por
    # padrão: aqui a chave da instalação e o state.json precisam ser os mesmos
    # para todas as instâncias, e "data" é o diretório deste aplicativo que
    # tem essa propriedade.
    export REMOTEID_HOME="$REMOTEID/estado"
    # O diagnóstico dele vai junto do estado, e por dois motivos.
    #
    # O primeiro é achá-lo: o RemoteID grava um JSONL por execução, e as
    # execuções que mais interessam são as do MÓDULO, que roda dentro da
    # ponte do navegador ou de um assinador — instâncias diferentes desta.
    # Num caminho compartilhado, `adv-br-remoteid diagnostico` lê todas.
    #
    # O segundo é não vazar. O RemoteID já redige o que grava: senha, PIN e
    # OTP nunca entram, e token só aparece como impressão digital. Mas o
    # arquivo IDENTIFICA o titular do certificado, e o "Relatar um problema"
    # deste aplicativo varre $XDG_DATA_HOME/logs e manda o
    # que achar. Por isso o diagnóstico do RemoteID fica FORA de lá: quem o
    # envia é a pessoa, de propósito, por canal privado. Ver ui/relator.py
    # e a seção de diagnóstico do README do RemoteID-linux.
    export REMOTEID_DIAG_DIR="$REMOTEID/estado/diag"
    mkdir -p "$REMOTEID_HOME" "$REMOTEID_DIAG_DIR" 2>/dev/null || true
fi
