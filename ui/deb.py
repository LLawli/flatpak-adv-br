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


def extrair_tar(dados, destino, mapa, cortar=0):
    """O mesmo que `extrair`, para um .tar.gz solto em vez de um .deb.

    `cortar` diz quantos componentes do caminho ignorar, como o
    --strip-components do tar. Serve para o pacote cujo diretorio raiz carrega
    a versao no nome (o JRE do Adoptium, por exemplo): sem isso, atualizar a
    versao mudaria todos os caminhos do catalogo.
    """
    with tarfile.open(fileobj=io.BytesIO(dados), mode="r:*") as tar:
        return _extrair_de(tar, destino, mapa, cortar)


def extrair(dados, destino, mapa):
    """Extrai de um .deb os caminhos de `mapa` ({de: para}), sob `destino`.

    Uma origem terminada em "/" e' um diretorio: tudo o que estiver sob ela e'
    copiado, preservando a estrutura. E' o que o SerproID exige, que traz um
    aplicativo inteiro com o JRE dele, e o que seria insano listar arquivo a
    arquivo.

    Devolve a lista do que foi escrito. Um caminho pedido e ausente e' erro:
    significa que o pacote do fabricante mudou de forma, e seguir em frente
    produziria uma instalacao que so' falha na hora de assinar.
    """
    with abrir_data(dados) as tar:
        return _extrair_de(tar, destino, mapa)


def _extrair_de(tar, destino, mapa, cortar=0):
    escritos = []
    if True:
        membros_por_nome = {}
        for m in tar.getmembers():
            nome = m.name.lstrip("./")
            if cortar:
                partes = nome.split("/")[cortar:]
                if not partes:
                    continue
                nome = "/".join(partes)
            membros_por_nome[nome] = m

        for origem, relativo in mapa.items():
            limpa = origem.lstrip("./")
            if origem.endswith("/"):
                escritos += _extrair_arvore(tar, membros_por_nome, limpa,
                                            os.path.join(destino, relativo))
                continue

            membro = membros_por_nome.get(limpa)
            if membro is None or not membro.isfile():
                raise DebInvalido("o pacote nao traz %s" % origem)
            escritos.append(_extrair_arquivo(tar, membro,
                                             os.path.join(destino, relativo)))
    return escritos


def _extrair_arquivo(tar, membro, caminho):
    os.makedirs(os.path.dirname(caminho), exist_ok=True)
    extraido = tar.extractfile(membro)
    if extraido is None:
        raise DebInvalido("nao consegui ler %s" % membro.name)
    with extraido, open(caminho, "wb") as saida:
        saida.write(extraido.read())
    # O modo do .deb vale: um driver sem bit de execucao nao carrega, e um .so
    # sem leitura para o usuario nao abre.
    os.chmod(caminho, membro.mode | 0o600)
    return caminho


def _extrair_arvore(tar, membros_por_nome, prefixo, destino):
    """Copia tudo o que estiver sob `prefixo`, preservando a estrutura."""
    escritos = []
    for nome, membro in membros_por_nome.items():
        if not nome.startswith(prefixo):
            continue
        relativo = nome[len(prefixo):].lstrip("/")
        if not relativo:
            continue

        alvo = os.path.join(destino, relativo)
        if membro.isdir():
            os.makedirs(alvo, exist_ok=True)
        elif membro.isfile():
            escritos.append(_extrair_arquivo(tar, membro, alvo))
        elif membro.issym():
            # O JRE que o SerproID traz vem cheio de links. Um que aponte para
            # fora da arvore seria um caminho de escape, e nao entra.
            os.makedirs(os.path.dirname(alvo), exist_ok=True)
            aponta = os.path.normpath(
                os.path.join(os.path.dirname(alvo), membro.linkname))
            if not aponta.startswith(os.path.normpath(destino)):
                continue
            if os.path.lexists(alvo):
                os.unlink(alvo)
            os.symlink(membro.linkname, alvo)
            escritos.append(alvo)

    if not escritos:
        raise DebInvalido("o pacote nao traz nada sob %s" % prefixo)
    return escritos
