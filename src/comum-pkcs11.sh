#!/bin/bash
# Monta a camada PKCS#11 dentro do sandbox. Não roda sozinho: é carregado com
# "." pelos comandos deste pacote.
#
# shellcheck disable=SC2034
# (as variáveis daqui são usadas por quem carrega este arquivo, não por ele.)

MODULOS_SANDBOX=/etc/pkcs11/modules
MODULOS_APP=/app/share/adv-br/pkcs11-modules
DRIVERS=/app/lib/pkcs11/drivers
# Assinadores e aplicativos também são extensões: o pacote base traz só o que
# pode ser redistribuído e o que todo mundo usa. Ver assinadores/README.md.
ASSINADORES=/app/lib/assinadores
APPS=/app/lib/apps
# Caminhos curtos e estáveis para quem precisa digitar um caminho de módulo na
# interface de um assinador. Criados no tmpfs da raiz do sandbox a cada
# execução, e por isso independentes da versão da extensão de driver.
ATALHOS=/pkcs11
# A biblioteca única que o PJeOffice carrega, e o alvo dos atalhos.
SHIM=/app/lib/pkcs11/pkcs11.so

# Caminho do p11-kit-proxy do runtime, procurado e não deduzido do triplet:
# 'gcc -dumpmachine' no SDK dá x86_64-unknown-linux-gnu, mas o diretório de
# bibliotecas do runtime é x86_64-linux-gnu.
caminho_do_proxy() {
    local candidato
    for candidato in /usr/lib/*/p11-kit-proxy.so /usr/lib/p11-kit-proxy.so; do
        [ -e "$candidato" ] && { printf '%s\n' "$candidato"; return 0; }
    done
    return 1
}

# LD_LIBRARY_PATH e preparar.sh de cada extensão instalada: de driver, de
# assinador ou de aplicativo. Precisa rodar antes de qualquer coisa carregar um
# módulo PKCS#11 ou abrir um assinador.
#
# preparar.sh existe por causa do SerproID: a libneoidp11.so faz readdir em
# ~/.config/serproid/certificados assim que é carregada e derruba com SIGSEGV
# quem a carregou se o diretório não existir.
preparar_drivers() {
    local driver nome
    for driver in "$DRIVERS"/*/ "$ASSINADORES"/*/ "$APPS"/*/; do
        [ -d "$driver" ] || continue
        nome=$(basename "$driver")
        if [ -d "$driver/lib" ]; then
            export LD_LIBRARY_PATH="$driver/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
        fi
        if [ -x "$driver/preparar.sh" ]; then
            # Um driver mal preparado não pode impedir os outros de subir.
            "$driver/preparar.sh" >&2 || echo "adv-br: preparar.sh de $nome falhou" >&2
        fi
    done
    preparar_remoteid
}

# O RemoteID precisa de uma coisa que preparar.sh não sabe fazer: exportar.
#
# Ele é certificado em NUVEM, e o módulo PKCS#11 dele não fala com a Certisign.
# O módulo manda o digest para o aplicativo do RemoteID por um socket UNIX, e é
# o aplicativo que pede o PIN e o código do autenticador. Os dois precisam
# concordar sobre onde está esse socket, e o padrão do RemoteID
# ($XDG_RUNTIME_DIR/remoteid.sock) não serve: o módulo é aberto numa instância
# deste Flatpak — a ponte do navegador, um assinador, o PJeOffice — e o
# aplicativo roda em OUTRA, com um $XDG_RUNTIME_DIR próprio. Cada lado criaria
# o seu, e o sintoma só apareceria na hora de assinar.
#
# O que atravessa é o diretório de dados, que é o mesmo arquivo em todas as
# instâncias. O REMOTEID_SOCKET vence qualquer padrão do RemoteID, e é por isso
# que isto está AQUI e não num preparar.sh da extensão: o preparar.sh é
# executado, não incluído, e variável exportada nele morre com ele.
#
# É inofensivo quando o RemoteID não está instalado: são duas variáveis que
# ninguém lê. Ver drivers/io.github.llawli.AdvBr.Driver.RemoteID.yml.
preparar_remoteid() {
    local dados teste
    dados="${XDG_DATA_HOME:-$HOME/.local/share}/remoteid"
    mkdir -p "$dados" 2>/dev/null ||
        echo "adv-br: não deu para criar $dados; o RemoteID pode não assinar." >&2

    # O modo de teste é um ARQUIVO, e não uma variável: ele precisa valer para
    # processos que ninguém lança à mão, como a ponte que o p11-kit do host
    # inicia sob demanda e o assinador que o navegador executa. Nenhum deles
    # herda o ambiente de um terminal.
    teste=""
    if [ -s "$dados/TEST_URL" ]; then
        teste=$(head -1 "$dados/TEST_URL" | tr -d '[:space:]')
        # O arquivo é gravável por quem tiver acesso aos dados do aplicativo, e
        # o que estiver nele vira variável de ambiente. Só uma URL http(s) sem
        # caractere de shell atravessa.
        case "$teste" in
            http://*|https://*)
                case "$teste" in *[!A-Za-z0-9:/._-]*) teste="" ;; esac
                ;;
            *) teste="" ;;
        esac
        [ -n "$teste" ] ||
            echo "adv-br: $dados/TEST_URL não é uma URL aceitável; modo de teste ignorado." >&2
    fi

    if [ -n "$teste" ]; then
        export TEST_URL="$teste"
        # Socket separado do de produção: um aplicativo aberto em modo de teste
        # não pode responder por um pedido de assinatura de verdade.
        export REMOTEID_SOCKET="$dados/teste/remoteid.sock"
        mkdir -p "$dados/teste" 2>/dev/null || true
        # Com TEST_URL, o RemoteID reloca o estado para /tmp/remoteid-teste sem
        # consultar REMOTEID_HOME. E /tmp, no sandbox, é tmpfs de cada
        # instância: o estado gravado pela linha de comando não chegaria ao
        # aplicativo nem ao módulo. A raiz do sandbox é gravável, então o
        # caminho que ele espera vira um link e some com a execução.
        if [ -d /tmp/remoteid-teste ] && [ ! -L /tmp/remoteid-teste ]; then
            echo "adv-br: /tmp/remoteid-teste já existe como diretório; o modo de teste vai gravar num lugar que não persiste." >&2
        else
            ln -sfn "$dados/teste" /tmp/remoteid-teste 2>/dev/null ||
                echo "adv-br: não consegui ligar /tmp/remoteid-teste." >&2
        fi
    else
        export REMOTEID_SOCKET="$dados/remoteid.sock"
        export REMOTEID_HOME="$dados/estado"
        # O diagnóstico dele vai junto do estado, e por dois motivos.
        #
        # O primeiro é achá-lo: o RemoteID grava um JSONL por execução, e as
        # execuções que mais interessam são as do MÓDULO, que roda dentro da
        # ponte do navegador ou de um assinador — instâncias diferentes desta.
        # Num caminho compartilhado, `adv-br-remoteid diagnostico` lê todas.
        #
        # O segundo é o relato de erro. O RemoteID já redige o que grava:
        # senha, PIN e OTP nunca entram, e token só aparece como impressão
        # digital. Ainda assim o arquivo IDENTIFICA o titular, então ele fica
        # fora de $XDG_DATA_HOME/logs, que é o que o
        # "Relatar um problema" varre por padrão.
        #
        # Enquanto o RemoteID está em fase de teste, o relato o inclui mesmo
        # assim, de um caminho próprio e com teto de tamanho: os dois primeiros
        # relatos de quem foi usá-lo chegaram inconclusivos sem ele. Ver
        # ui/diagnostico.py e tests/prova-relato.py.
        export REMOTEID_DIAG_DIR="$dados/estado/diag"
        mkdir -p "$REMOTEID_HOME" "$REMOTEID_DIAG_DIR" 2>/dev/null || true
    fi
}

# Imprime, uma por linha, "rótulo<TAB>caminho" de cada módulo PKCS#11 que este
# pacote enxerga: o OpenSC que vem embutido e o de cada extensão de driver.
#
# É desta lista que sai tudo o mais: os .module do host, os atalhos em /pkcs11
# e o que o diagnóstico mostra. Ela existe para que a resposta à pergunta
# "quais drivers há aqui dentro?" venha do pacote, e não de uma tabela repetida
# em cada script.
listar_modulos() {
    local modulo biblioteca driver nome
    for modulo in "$MODULOS_APP"/*.module; do
        [ -e "$modulo" ] || continue
        biblioteca=$(sed -n 's/^module:[[:space:]]*//p' "$modulo" | head -1)
        [ -e "$biblioteca" ] || continue
        printf '%s\t%s\n' "$(basename "$modulo" .module)" "$biblioteca"
    done
    for driver in "$DRIVERS"/*/; do
        [ -d "$driver" ] || continue
        nome=$(basename "$driver")
        for biblioteca in "$driver"pkcs11/*.so; do
            [ -e "$biblioteca" ] || continue
            printf '%s\t%s\n' "$nome-$(basename "$biblioteca" .so)" "$biblioteca"
        done
    done
}

# Registra os módulos no p11-kit DESTE sandbox, para que o p11-kit-proxy
# responda por todos eles de uma vez. É o que os assinadores consomem.
#
# /etc é um tmpfs recriado a cada execução, então isto é refeito em todo
# começo, e é o que permite um pacote somente-leitura responder por drivers
# instalados depois, como extensão.
registrar_modulos_no_sandbox() {
    [ -w "$MODULOS_SANDBOX" ] || {
        echo "adv-br: $MODULOS_SANDBOX não é gravável; nenhum driver registrado." >&2
        return 0
    }
    local rotulo biblioteca
    while IFS=$'\t' read -r rotulo biblioteca; do
        # critical: no. Com mais de um driver instalado, um deles recusar o
        # cartão presente é o caso comum, não a exceção.
        printf 'module: %s\ncritical: no\n' "$biblioteca" \
            > "$MODULOS_SANDBOX/$rotulo.module"
    done < <(listar_modulos)
}

# Quantos módulos estão registrados e quantos o p11-kit realmente carrega.
#
# Um .module cujo driver não carrega simplesmente SOME da listagem, sem erro,
# porque todos são registrados com 'critical: no'. A diferença entre os dois
# números é o único sinal de que um driver não subiu: foi assim que se
# descobriu o SerproID falhando antes de existir o diretório de certificados
# que ele exige. Contar só os carregados, e comparar com um número fixo, não
# detecta nada.
contar_modulos() {
    local registrados carregados
    registrados=$(ls "$MODULOS_SANDBOX"/*.module 2>/dev/null | wc -l)
    carregados=$(p11-kit list-modules 2>/dev/null | grep -c '^module:')
    printf '%s\t%s\n' "$registrados" "$carregados"
}

# Atalhos em /pkcs11, para a aba "Cripto Dispositivos" das extensões.
#
# As opções prontas dessas extensões apontam para /usr/lib, que aqui pertence
# ao runtime e é somente leitura: não há como fazer o caminho de fábrica
# existir. O que dá para oferecer é um caminho curto, estável e digitável.
#
# /pkcs11/adv-br.so é um caminho só que responde por todos os drivers
# registrados, inclusive os que forem instalados depois.
#
# Ele aponta para o SHIM, e não direto para o p11-kit-proxy do runtime, porque
# há quem canonize o caminho antes de guardá-lo: o PJeOffice chama
# library.toRealPath() e grava o resultado em
# ~/.pjeoffice-pro/pjeoffice-pro.config. Com o proxy no fim do symlink, o que
# ficaria gravado é libp11-kit.so.0.4.10, e a primeira atualização de runtime
# que mude esse número tira o driver do usuário sem dizer nada. O shim é
# arquivo regular: o caminho real dele é ele mesmo. Ver src/pkcs11-shim.c.
criar_atalhos() {
    local alvo rotulo biblioteca
    mkdir -p "$ATALHOS" 2>/dev/null || return 0
    if [ -e "$SHIM" ]; then
        alvo=$SHIM
    else
        alvo=$(caminho_do_proxy) || return 0
    fi
    ln -sfn "$alvo" "$ATALHOS/adv-br.so"
    while IFS=$'\t' read -r rotulo biblioteca; do
        ln -sfn "$biblioteca" "$ATALHOS/$rotulo.so"
    done < <(listar_modulos)
}
