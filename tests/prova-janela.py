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


def caminho_declarado():
    """A string de ATALHO_DOS_ASSINADORES, lida sem importar o módulo."""
    arvore = ast.parse(open("ui/pkcs11.py", encoding="utf-8").read())
    for no in arvore.body:
        if not isinstance(no, ast.Assign):
            continue
        for alvo in no.targets:
            if isinstance(alvo, ast.Name) and alvo.id == "ATALHO_DOS_ASSINADORES":
                return ast.literal_eval(no.value)
    raise SystemExit("ui/pkcs11.py não declara ATALHO_DOS_ASSINADORES")


def provar_caminho_do_driver():
    """O caminho tem de ser o mesmo nas duas pontas, e ele tem duas.

    Quem CRIA o link é um script de shell; quem DIZ o caminho à pessoa é a
    janela. Divergir aqui não quebra nada visível: a janela mostra um caminho, a
    extensão do assinador não encontra nada nele, e a conclusão de quem usa é
    que o token não funciona.
    """
    caminho = caminho_declarado()
    falhas = 0

    shell = open("ui/preparar-drivers.sh", encoding="utf-8").read()
    if caminho in shell:
        print("  ok   preparar-drivers.sh cria %s" % caminho)
    else:
        print("  ERRO preparar-drivers.sh não menciona %s" % caminho)
        falhas += 1

    janela = open(CAMINHO, encoding="utf-8").read()
    if "pkcs11.ATALHO_DOS_ASSINADORES" in janela:
        print("  ok   a janela usa a constante, e não a string repetida")
    else:
        print("  ERRO a janela não usa pkcs11.ATALHO_DOS_ASSINADORES")
        falhas += 1

    return falhas


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

    falhas += provar_caminho_do_driver()

    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
