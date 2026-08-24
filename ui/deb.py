"""Lê um pacote .deb sem depender do `ar`, que o runtime não traz.

Um .deb é um arquivo `ar` com três membros, nesta ordem: `debian-binary`,
`control.tar.*` e `data.tar.*`. O que interessa aqui é o último, e o formato
`ar` é simples o bastante para ser lido em algumas linhas: uma assinatura de
8 bytes e, por membro, um cabeçalho de 60 bytes com o nome nos 16 primeiros e o
tamanho a partir do byte 48, ambos em texto decimal.

Alternativa descartada: chamar `ar` por flatpak-spawn no host. Isso traria de
volta a permissão que este projeto existe para não pedir.
"""
import io
import lzma
import os
import shutil
import subprocess
import tarfile

ASSINATURA = b"!<arch>\n"


class DebInvalido(Exception):
    pass


def _descomprimir_zstd(dados):
    """Descomprime zstd chamando o binário do runtime.

    O tarfile só passou a ler zstd no Python 3.14 (PEP 784), e o runtime traz
    o 3.13. O binário `zstd` está lá, então é ele que faz o trabalho: é isso
    ou embutir uma dependência nova por causa de um formato que o SafeSign
    usa e os outros pacotes não.
    """
    if shutil.which("zstd") is None:
        raise DebInvalido(
            "este pacote vem em data.tar.zst e não há zstd neste ambiente")
    resultado = subprocess.run(
        ["zstd", "-d", "-c"], input=dados,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    if resultado.returncode != 0:
        raise DebInvalido(
            "zstd falhou: %s" % resultado.stderr.decode("utf-8", "replace").strip())
    return resultado.stdout


def membros(dados):
    """Itera (nome, conteúdo) de cada membro do arquivo ar."""
    if not dados.startswith(ASSINATURA):
        raise DebInvalido("não começa com a assinatura de um arquivo ar")

    posicao = len(ASSINATURA)
    while posicao + 60 <= len(dados):
        cabecalho = dados[posicao:posicao + 60]
        nome = cabecalho[0:16].decode("ascii", "replace").strip()
        # O tamanho vem em texto decimal; um cabeçalho truncado ou corrompido
        # produziria um int() que estoura, e é melhor dizer isso do que seguir
        # lendo lixo.
        try:
            tamanho = int(cabecalho[48:58].decode("ascii").strip())
        except ValueError as erro:
            raise DebInvalido("cabeçalho ar ilegível em %d" % posicao) from erro

        inicio = posicao + 60
        conteudo = dados[inicio:inicio + tamanho]
        # O formato exige que cada membro comece em byte par.
        posicao = inicio + tamanho + (tamanho % 2)
        yield nome.rstrip("/"), conteudo


def abrir_data(dados):
    """Devolve o tarfile do membro data.tar.* de um .deb."""
    for nome, conteudo in membros(dados):
        if not nome.startswith("data.tar"):
            continue
        if nome.endswith(".zst"):
            return tarfile.open(fileobj=io.BytesIO(_descomprimir_zstd(conteudo)))
        if nome.endswith(".xz"):
            return tarfile.open(fileobj=io.BytesIO(lzma.decompress(conteudo)))
        # .gz e .bz2 o próprio tarfile resolve pelo modo "r:*".
        return tarfile.open(fileobj=io.BytesIO(conteudo))
    raise DebInvalido("não achei o membro data.tar.* no pacote")


def extrair(dados, destino, mapa):
    """Extrai de um .deb os caminhos de `mapa` ({de: para}), sob `destino`.

    Devolve a lista do que foi escrito. Um caminho pedido e ausente é erro:
    significa que o pacote do fabricante mudou de forma, e seguir em frente
    produziria uma instalação que só falha na hora de assinar.
    """
    escritos = []
    with abrir_data(dados) as tar:
        disponiveis = {m.name.lstrip("./"): m for m in tar.getmembers()}
        for origem, relativo in mapa.items():
            membro = disponiveis.get(origem.lstrip("./"))
            if membro is None or not membro.isfile():
                raise DebInvalido("o pacote não traz %s" % origem)

            caminho = os.path.join(destino, relativo)
            os.makedirs(os.path.dirname(caminho), exist_ok=True)
            extraido = tar.extractfile(membro)
            if extraido is None:
                raise DebInvalido("não consegui ler %s" % origem)
            with extraido, open(caminho, "wb") as saida:
                saida.write(extraido.read())
            # O modo do .deb vale: um driver sem bit de execução não carrega, e
            # um .so sem leitura para o usuário não abre.
            os.chmod(caminho, membro.mode | 0o600)
            escritos.append(caminho)
    return escritos
