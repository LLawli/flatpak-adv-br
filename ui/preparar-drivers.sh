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
