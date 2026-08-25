"""Os lançadores registram, e nenhum deles escreve em stdout.

Duas regras, e a segunda é a mais cara do projeto: a ponte PKCS#11 fala RPC do
p11-kit pelo stdout e os assinadores falam native messaging pelo stdout. Uma
linha de log ali corrompe o protocolo, e o sintoma no navegador é "o assinador
não respondeu" ou um token que some, sem nada que aponte para a causa.

A primeira regra é que cada lançador mande o próprio stderr para o arquivo do
seu módulo, e antes de escrever qualquer coisa: mensagem emitida antes da
inclusão se perde onde sempre se perdeu.
"""
import ast
import glob
import os
import re
import sys

LANCADORES = ["ui/adv-br-ui", "ui/adv-br-pkcs11", "ui/adv-br-assinador"]
INCLUSAO = ". /app/share/adv-br-ui/registro.sh"

# Uma linha que escreve, sem mandar para o stderr. Não pretende entender shell:
# pretende ser barulhenta o bastante para ninguém acrescentar um echo distraído.
ESCREVE = re.compile(r"^\s*(echo|printf)\s")
PARA_STDERR = re.compile(r">&\s*2")


def conferir(caminho):
    problemas = []
    incluiu = False
    definiu = False

    with open(caminho, encoding="utf-8") as arquivo:
        for numero, linha in enumerate(arquivo, 1):
            nua = linha.split("#", 1)[0] if not linha.lstrip().startswith("#") else ""

            if INCLUSAO in linha:
                incluiu = True
            if "ADV_BR_MODULO=" in nua:
                definiu = True

            if ESCREVE.match(nua) and not PARA_STDERR.search(nua):
                problemas.append(
                    "%s:%d escreve em stdout: %s" % (caminho, numero, linha.strip()))
            elif ESCREVE.match(nua) and not incluiu:
                problemas.append(
                    "%s:%d escreve antes de incluir o registro" % (caminho, numero))

    if not definiu:
        problemas.append("%s não define ADV_BR_MODULO" % caminho)
    if not incluiu:
        problemas.append("%s não inclui o registro.sh" % caminho)
    return problemas


def conferir_lancador_de_componente():
    """O lançador que o instalador escreve também precisa registrar."""
    sys.path.insert(0, "ui")
    import catalogo  # noqa: E402
    import instalador  # noqa: E402

    for componente in catalogo.CATALOGO:
        if not componente.lancador:
            continue
        corpo = ("#!/bin/sh\nset -eu\n"
                 'ADV_BR_MODULO=app-%s\n' % componente.chave
                 + '. /app/share/adv-br-ui/registro.sh\n')
        # O que interessa é que instalador._escrever_lancador produza isto; o
        # arquivo real só existe depois de instalar, então se confere a fonte.
        fonte = open("ui/instalador.py", encoding="utf-8").read()
        if "ADV_BR_MODULO=app-" not in fonte or "registro.sh" not in fonte:
            return ["ui/instalador.py não põe o registro no lançador do componente"]
        del corpo
        break
    return []


def main():
    if len(LANCADORES) < 3:
        print("  ERRO a lista de lançadores encolheu")
        return 1

    problemas = []
    for caminho in LANCADORES:
        if not os.path.exists(caminho):
            problemas.append("%s não existe" % caminho)
            continue
        problemas += conferir(caminho)
    problemas += conferir_lancador_de_componente()

    for problema in problemas:
        print("  ERRO %s" % problema)
    if not problemas:
        print("  ok  %d lançadores registram e nenhum escreve em stdout"
              % len(LANCADORES))
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
