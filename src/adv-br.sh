#!/bin/bash
# Comando padrão do pacote: conta o que existe aqui dentro.
#
# Serve para separar dois problemas que, vistos do navegador, se parecem: "o
# pacote não enxerga o token" e "a ponte até o navegador não está de pé". Este
# comando só olha o lado de dentro; quem olha a travessia é o ./diagnostico.sh
# do repositório.
set -uo pipefail

. /app/share/adv-br/comum-pkcs11.sh

titulo() { printf '\n\033[1;36m━━ %s\033[0m\n' "$*"; }
ok()     { printf '\033[1;32m ✓\033[0m %s\n' "$*"; }
aviso()  { printf '\033[1;33m !\033[0m %s\n' "$*"; }
falha()  { printf '\033[1;31m ✗\033[0m %s\n' "$*"; }

preparar_drivers
registrar_modulos_no_sandbox
criar_atalhos

titulo "Assinadores e aplicativos instalados"
achou=0
for extensao in "$ASSINADORES"/*/ "$APPS"/*/; do
    [ -d "$extensao" ] || continue
    achou=1
    ok "$(basename "$extensao")"
done
if [ "$achou" = 0 ]; then
    aviso "nenhum. O pacote base traz só o OpenSC e a ponte; assinador e
   aplicativo se instalam quando você vai usar:
       ./instalar.sh --with-webpki       assinar com o componente da Lacuna
       ./instalar.sh --with-websigner    assinar nos sistemas SAJ
       ./instalar.sh --with-certisign    portal de assinatura da OAB
       ./instalar.sh --with-pjeoffice    assinar no PJe (CNJ)"
fi

titulo "Módulos PKCS#11 visíveis aqui dentro"
achou=0
while IFS=$'\t' read -r rotulo biblioteca; do
    achou=1
    ok "$rotulo → $biblioteca"
done < <(listar_modulos)
[ "$achou" = 1 ] || falha "nenhum módulo, nem o OpenSC do pacote. Algo saiu errado no build."

titulo "Drivers instalados como extensão"
achou=0
for driver in "$DRIVERS"/*/; do
    [ -d "$driver" ] || continue
    achou=1
    ok "$(basename "$driver")"
done
[ "$achou" = 1 ] || aviso "nenhuma. Só o OpenSC do pacote responde pelos tokens.
   Para instalar:  ./instalar.sh --with-safesign  (ou --with-safenet, --with-serproid)"

titulo "Leitora e token"
if [ -S /run/pcscd/pcscd.comm ]; then
    ok "socket do pcscd do host visível no sandbox"
else
    falha "socket do pcscd não está visível: /run/pcscd/pcscd.comm não existe.
   No host:  sudo systemctl enable --now pcscd.socket"
fi

titulo "O que o p11-kit deste sandbox enxerga"
# 'couldn't load token info' aqui não é falha: é o que o p11-kit imprime quando
# um driver diz que o cartão presente na leitora não é dele, o que acontece
# sempre que há mais de um driver instalado.
p11-kit list-modules 2>&1 | sed 's/^/   /'
printf '\n'
