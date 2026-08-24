#!/bin/bash
# Descreve, para o publicador do host, os assinadores instalados.
#
# Uma linha por assinador e por família de navegador:
#
#     <host-name>\t<familia>\t<comando-flatpak>\t<manifesto-json-numa-linha>
#
# O manifesto sai do arquivo que o próprio .deb do fabricante instalou, e que a
# extensão guardou em native-messaging/: quem responde por "quais extensões de
# navegador podem falar com este assinador" é ele, não este projeto. O
# publicador só troca o campo "path".
set -euo pipefail

. /app/share/adv-br/comum-pkcs11.sh

exec python3 - "$ASSINADORES" <<'PY'
import glob
import json
import os
import sys

raiz = sys.argv[1]

for arquivo in sorted(glob.glob(os.path.join(raiz, "*", "native-messaging", "*.json"))):
    # <host-name>.<familia>.json
    nome, familia, _ = os.path.basename(arquivo).rsplit(".", 2)

    # O comando é o do executável que a extensão instalou em bin/, com o
    # prefixo do pacote: bin/webpki → adv-br-webpki. Assim a tabela de
    # "qual comando lança qual assinador" não existe em lugar nenhum — ela é
    # a própria extensão.
    extensao = os.path.dirname(os.path.dirname(arquivo))
    binarios = [os.path.basename(b) for b in glob.glob(os.path.join(extensao, "bin", "*"))
                if os.access(b, os.X_OK)]
    if not binarios:
        continue
    comando = "adv-br-" + binarios[0]

    with open(arquivo, encoding="utf-8") as f:
        manifesto = json.load(f)
    print("\t".join([nome, familia, comando,
                     json.dumps(manifesto, ensure_ascii=False)]))
PY
