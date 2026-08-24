#!/bin/bash
# Descreve, para o publicador do host, os assinadores que este pacote traz.
#
# Uma linha por assinador e por família de navegador:
#
#     <host-name>\t<familia>\t<comando-flatpak>\t<manifesto-json-numa-linha>
#
# O manifesto sai dos arquivos que os próprios .deb dos fabricantes
# instalaram: quem responde por "quais extensões de navegador podem falar com
# este assinador" é o fabricante, não uma tabela copiada para dentro deste
# projeto. O publicador só troca o campo "path".
set -euo pipefail

exec python3 - <<'PY'
import glob
import json
import os

# A única coisa que este projeto acrescenta: qual comando do Flatpak lança cada
# assinador. O resto do manifesto é do fabricante.
COMANDOS = {
    "com.lacunasoftware.webpki": "adv-br-webpki",
    "br.com.softplan.webpki": "adv-br-websigner",
    "br.com.certisign.websigner": "adv-br-certisign",
}

for arquivo in sorted(glob.glob("/app/share/adv-br/native-messaging/*.json")):
    # <host-name>.<familia>.json
    nome, familia, _ = os.path.basename(arquivo).rsplit(".", 2)
    comando = COMANDOS.get(nome)
    if comando is None:
        continue
    with open(arquivo, encoding="utf-8") as f:
        manifesto = json.load(f)
    print("\t".join([nome, familia, comando,
                     json.dumps(manifesto, ensure_ascii=False)]))
PY
