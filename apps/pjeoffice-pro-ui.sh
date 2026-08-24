#!/bin/bash
# Lançador do PJeOffice Pro dentro do aplicativo com interface.
#
# O PJeOffice é o assinador do CNJ: um programa Java que sobe um servidor em
# 127.0.0.1:8800 e conversa com os sistemas do Judiciário pelo navegador. Ele
# está aqui pelo mesmo motivo dos assinadores: é onde os drivers de token
# funcionam.
#
# Ele descobre driver de duas formas, e as duas são inúteis num sandbox:
# varrendo nomes fixos em /usr/lib, que é o runtime e é somente leitura, e
# lendo a variável PKCS11_DRIVER como diretório, de onde carrega um
# "pkcs11.so". A segunda é a saída, e é um caminho só: daí o shim do aplicativo
# base (ver src/pkcs11-shim.c), que repassa ao p11-kit-proxy e assim responde
# por todos os drivers de uma vez, inclusive pelos que forem instalados depois.
set -euo pipefail

AQUI=$(cd -- "$(dirname -- "$0")/.." && pwd)
PJE_HOME="$AQUI/share/pjeoffice-pro"

# Ver ui/preparar-drivers.sh: bibliotecas de apoio dos componentes e os
# remendos que os drivers proprietários exigem antes de serem abertos.
. /app/share/adv-br-ui/preparar-drivers.sh

# Os drivers que a pessoa instalou pela janela são dados do aplicativo, e o
# p11-kit não sabe deles até que alguém escreva os .module. Quem escreve é a
# janela, ao abrir; aqui não dá para contar com ela ter sido aberta antes.
PYTHONPATH=/app/share/adv-br-ui python3 -c 'import pkcs11; pkcs11.registrar()' \
    || echo "adv-br: não deu para registrar os drivers no p11-kit." >&2

export PKCS11_DRIVER=/app/lib/pkcs11

# O AWT do Java não fala Wayland: roda por XWayland. Sem isto, o KWin e outros
# compositores reparentam a janela e o Swing desenha a decoração no lugar
# errado.
export _JAVA_AWT_WM_NONREPARENTING=1

# O SafeNet derruba a JVM no ENCERRAMENTO, não em uso: depois que o SunPKCS11
# foi inicializado, sair dá SIGSEGV dentro de SCardCancel, numa thread nativa
# que o próprio driver criou. Em uso não há problema, o assinador opera
# normalmente. Sem esta opção a JVM ainda escreve um core dump antes de morrer,
# e a espera faz o crash parecer travamento: custou uma rodada inteira de
# diagnóstico achando que o driver pendurava.
exec "$AQUI/jre/bin/java" \
    -XX:-CreateCoredumpOnCrash \
    -XX:+UseG1GC \
    -XX:MinHeapFreeRatio=3 \
    -XX:MaxHeapFreeRatio=3 \
    -Xms20m \
    -Xmx2048m \
    -Dpjeoffice_home="$PJE_HOME/" \
    -Dffmpeg_home="$PJE_HOME/" \
    -Dpjeoffice_looksandfeels=Metal \
    -Dcutplayer4j_looksandfeels=Nimbus \
    -jar "$PJE_HOME/pjeoffice-pro.jar" \
    "$@"
