"""Exercita as decisões da interface que não dependem de abrir janela.

O GTK não entra aqui: a função sob teste é extraída do módulo por análise
sintática. É feio, e é de propósito. A alternativa seria subir uma Adw
.Application num runner sem sessão gráfica, e um teste que não roda no CI não
protege nada.
"""
import ast
import sys

CAMINHO = "ui/janela.py"


def carregar(nome):
    arvore = ast.parse(open(CAMINHO, encoding="utf-8").read())
    corpo = [no for no in arvore.body
             if isinstance(no, ast.FunctionDef) and no.name == nome]
    if not corpo:
        raise SystemExit("%s não tem a função %s" % (CAMINHO, nome))
    espaco = {}
    exec(compile(ast.Module(corpo, []), CAMINHO, "exec"), espaco)  # noqa: S102
    return espaco[nome]


def main():
    decidir = carregar("decidir")

    # (o que o botão mostrava, o que o disco diz) -> o que fazer
    #
    # As duas últimas linhas são o bug que originou este teste: com a janela
    # aberta desde antes de o componente mudar por fora, clicar em "Remover"
    # reinstalava, porque a ação vinha do disco e o rótulo vinha da memória.
    casos = [
        ((None, True), "remover"),
        ((None, False), "instalar"),
        ((True, True), "remover"),
        ((False, False), "instalar"),
        ((True, False), "sincronizar"),
        ((False, True), "sincronizar"),
    ]

    falhas = 0
    for (desenhado, real), esperado in casos:
        obtido = decidir(desenhado, real)
        marca = "ok " if obtido == esperado else "ERRO"
        print("  %s desenhado=%-5s real=%-5s -> %s"
              % (marca, desenhado, real, obtido))
        falhas += obtido != esperado

    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
