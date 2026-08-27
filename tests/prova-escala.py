"""A escala do monitor, do arquivo até a linha de comando da JVM.

O PJeOffice é Swing por XWayland e não descobre escala fracionária sozinho: num
monitor a 125% ou 150% ele sai borrado. A janela anota o número e o lançador o
repassa, e o que este teste exercita é o repasse, com um "java" de mentira que
só imprime o que recebeu.
"""
import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, "ui")
import catalogo  # noqa: E402
import escala  # noqa: E402


def montar(raiz):
    """Um componente de mentira, com o java trocado por um eco."""
    binario = os.path.join(raiz, "jre", "bin")
    os.makedirs(binario, exist_ok=True)
    with open(os.path.join(binario, "java"), "w", encoding="utf-8") as arquivo:
        arquivo.write('#!/bin/sh\nprintf "%s\\n" "$@"\n')
    os.chmod(os.path.join(binario, "java"), 0o755)
    os.makedirs(os.path.join(raiz, "share", "pjeoffice-pro"), exist_ok=True)


def rodar(raiz, dados):
    componente = [c for c in catalogo.CATALOGO if c.chave == "pjeoffice"][0]
    corpo = ('#!/bin/sh\nset -eu\nCOMPONENTE=%s\n' % raiz) + componente.lancador
    # As duas linhas que só existem dentro do sandbox.
    corpo = corpo.replace(". /app/share/adv-br-ui/registro.sh", ":")
    corpo = corpo.replace("PYTHONPATH=/app/share/adv-br-ui python3 -c "
                          "'import pkcs11; pkcs11.registrar()'", "true")
    caminho = os.path.join(raiz, "lancador.sh")
    with open(caminho, "w", encoding="utf-8") as arquivo:
        arquivo.write(corpo)
    os.chmod(caminho, 0o755)
    ambiente = dict(os.environ, XDG_DATA_HOME=dados, HOME=dados)
    saida = subprocess.run([caminho], capture_output=True, text=True, env=ambiente)
    return saida.stdout.splitlines()


def main():
    base = tempfile.mkdtemp(prefix="prova-escala.")
    falhas = 0
    try:
        raiz = os.path.join(base, "componente")
        dados = os.path.join(base, "dados")
        os.makedirs(dados, exist_ok=True)
        montar(raiz)

        casos = [
            ("1.5", True, "escala fracionária chega à JVM"),
            ("2", True, "escala inteira chega à JVM"),
            ("", False, "sem escala anotada, nenhuma opção é passada"),
            ("1.5; rm -rf /", False, "valor com comando é recusado"),
            ("abc", False, "valor não numérico é recusado"),
        ]
        for valor, esperado, descricao in casos:
            arquivo = os.path.join(dados, "escala")
            if valor:
                with open(arquivo, "w", encoding="utf-8") as saida:
                    saida.write(valor + "\n")
            elif os.path.exists(arquivo):
                os.remove(arquivo)

            argumentos = rodar(raiz, dados)
            passou = [a for a in argumentos if "uiScale" in a]
            obtido = bool(passou)
            marca = "ok " if obtido == esperado else "ERRO"
            falhas += obtido != esperado
            print("  %s %-46s %s" % (marca, descricao,
                                     " ".join(passou) if passou else "(nenhuma)"))

        # O que a janela grava tem de ser o que o lançador lê.
        if escala.ARQUIVO.endswith("/escala"):
            print("  ok  janela e lançador combinam o nome do arquivo")
        else:
            print("  ERRO ui/escala.py grava em %s" % escala.ARQUIVO)
            falhas += 1

        return 1 if falhas else 0
    finally:
        shutil.rmtree(base, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
