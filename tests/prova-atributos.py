"""Procura self._alguma_coisa que não existe na classe.

Existe por causa de um bug real: ao remover um diálogo, o método que tratava a
resposta dele saiu junto, e dois outros diálogos ainda o usavam. Nada acusou.
O py_compile passa, o import passa, a janela abre, e a falha só aparece quando
alguém clica: o diálogo é construído, a exceção estoura antes do present(), e
dentro de um handler do GTK isso vira uma linha no stderr que ninguém lê. O
clique parece não fazer nada.

Só nomes com "_" na frente são conferidos. Os outros podem vir da classe base
(present, get_clipboard, set_content), e resolver herança de GObject por
análise sintática seria trocar um problema por outro.
"""
import ast
import glob
import sys


def analisar(caminho):
    """Devolve [(classe, atributo, linha)] dos privados usados e não definidos."""
    arvore = ast.parse(open(caminho, encoding="utf-8").read(), caminho)
    faltando = []

    for classe in [n for n in ast.walk(arvore) if isinstance(n, ast.ClassDef)]:
        definidos = set()
        usados = []

        for no in ast.walk(classe):
            if isinstance(no, (ast.FunctionDef, ast.AsyncFunctionDef)):
                definidos.add(no.name)
            elif isinstance(no, ast.Attribute) and isinstance(no.value, ast.Name) \
                    and no.value.id == "self":
                if isinstance(no.ctx, ast.Store):
                    definidos.add(no.attr)
                else:
                    usados.append((no.attr, no.lineno))

        for atributo, linha in usados:
            if atributo.startswith("_") and atributo not in definidos:
                faltando.append((classe.name, atributo, linha))
    return faltando


def main():
    arquivos = sorted(glob.glob("ui/*.py"))
    if len(arquivos) < 5:
        # Piso absoluto: um glob que não casa nada faria este teste passar sem
        # conferir arquivo nenhum.
        print("  ERRO esperava ao menos 5 módulos em ui/, achei %d" % len(arquivos))
        return 1

    falhas = 0
    for caminho in arquivos:
        for classe, atributo, linha in analisar(caminho):
            print("  ERRO %s:%d  %s.%s não existe"
                  % (caminho, linha, classe, atributo))
            falhas += 1
    if not falhas:
        print("  ok  %d módulos, nenhum self._atributo órfão" % len(arquivos))
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
