"""Um componente instalado sabe se é o que o catálogo pede hoje.

Até a 1.1.2, instalado era só "os arquivos estão lá". Um sha256 novo no
catálogo não alcançava quem já tinha o componente: o relato 13 chegou com o
RemoteID 0.3.0 debaixo do aplicativo 1.1.2, e a correção que a 1.1.2 levava
nunca foi instalada. O que este teste guarda:

- o que acabou de ser instalado está em dia;
- um sha256 diferente no catálogo o deixa desatualizado;
- sem registro (o que foi instalado antes de ele existir) também;
- uma atualização que falha no download deixa o anterior inteiro, registro
  incluído;
- e o que não está instalado não está desatualizado, para o botão "Atualizar"
  não aparecer ao lado de "Instalar".

O download é trocado por um tarball montado aqui: o que está sob teste é o
registro, e não a rede.
"""
import hashlib
import io
import os
import shutil
import sys
import tarfile
import tempfile

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(RAIZ, "ui"))

import catalogo  # noqa: E402
import instalador  # noqa: E402

CASAS = os.path.join(os.environ.get("XDG_CACHE_HOME")
                     or os.path.expanduser("~/.cache"), "adv-br-testes")


def _tarball(conteudo):
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz") as tar:
        dados = conteudo.encode()
        info = tarfile.TarInfo("pacote/dados/versao.txt")
        info.size = len(dados)
        tar.addfile(info, io.BytesIO(dados))
    return buffer.getvalue()


def _componente(pacote):
    return catalogo.Componente(
        chave="prova-atualizacao", nome="Prova", resumo="", detalhe="",
        tipo="driver", url="", sha256="", arquivos={}, tamanho=0,
        fontes=(catalogo.Fonte(
            url="https://exemplo.invalido/prova.tar.gz",
            sha256=hashlib.sha256(pacote).hexdigest(),
            arquivos={"dados/": "dados"}, formato="tar", cortar=1),))


def conferir():
    problemas = []
    os.makedirs(CASAS, exist_ok=True)
    raiz = tempfile.mkdtemp(prefix="prova-atualizacao-", dir=CASAS)
    salvo_dados = os.environ.get("XDG_DATA_HOME")
    baixar_de_verdade = instalador.baixar
    os.environ["XDG_DATA_HOME"] = raiz
    try:
        velho, novo = _tarball("0.3.0"), _tarball("0.3.1")
        antes, depois = _componente(velho), _componente(novo)
        registro = os.path.join(instalador.diretorio(antes), instalador.REGISTRO)

        # 1. Nada instalado: nem instalado, nem desatualizado.
        if instalador.desatualizado(antes):
            problemas.append("o que não está instalado aparece como desatualizado")

        # 2. O recém-instalado está em dia, e tem registro.
        instalador.baixar = lambda *_a, **_k: velho
        instalador.instalar(antes)
        if not instalador.instalado(antes):
            problemas.append("a instalação de prova não instalou")
        if not os.path.isfile(registro):
            problemas.append("a instalação não gravou o registro")
        if instalador.desatualizado(antes):
            problemas.append("o recém-instalado aparece como desatualizado")

        # 3. O catálogo pede outro pacote: desatualizado.
        if not instalador.desatualizado(depois):
            problemas.append("um sha256 novo no catálogo não deixou o "
                             "instalado desatualizado")

        # 4. Uma atualização cujo download falha não toca no anterior.
        def falha(*_a, **_k):
            raise ValueError("o arquivo baixado não confere com o esperado")
        instalador.baixar = falha
        try:
            instalador.instalar(depois)
            problemas.append("a atualização com download quebrado não falhou")
        except ValueError:
            pass
        if instalador.desatualizado(antes) or not instalador.instalado(antes):
            problemas.append("a atualização que falhou estragou o que já estava "
                             "instalado")

        # 5. A atualização que dá certo troca o conteúdo e o registro.
        instalador.baixar = lambda *_a, **_k: novo
        instalador.instalar(depois)
        caminho = os.path.join(instalador.diretorio(depois), "dados", "versao.txt")
        with open(caminho, encoding="utf-8") as arquivo:
            if arquivo.read() != "0.3.1":
                problemas.append("a atualização não trocou o conteúdo")
        if instalador.desatualizado(depois):
            problemas.append("depois de atualizar, continua desatualizado")

        # 6. Sem registro, ou com um registro ilegível, é desatualizado: é o
        #    que foi instalado antes de o registro existir.
        if os.path.exists(registro):
            os.remove(registro)
        if not instalador.desatualizado(depois):
            problemas.append("sem registro, não aparece como desatualizado")
        with open(registro, "w", encoding="utf-8") as arquivo:
            arquivo.write("{ isto não é json")
        if not instalador.desatualizado(depois):
            problemas.append("com registro ilegível, não aparece como desatualizado")
    finally:
        instalador.baixar = baixar_de_verdade
        if salvo_dados is None:
            os.environ.pop("XDG_DATA_HOME", None)
        else:
            os.environ["XDG_DATA_HOME"] = salvo_dados
        shutil.rmtree(raiz, ignore_errors=True)
    return problemas


def main():
    problemas = conferir()
    for problema in problemas:
        print("  " + problema, file=sys.stderr)
    if not problemas:
        print("  ok  o componente instalado sabe se está desatualizado")
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
