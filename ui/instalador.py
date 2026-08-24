"""Baixa, confere e instala um componente nos dados do aplicativo.

Onde: $XDG_DATA_HOME/componentes/<Chave>/, que dentro do sandbox é gravável e
de onde um .so carrega. Verificado antes de o desenho ser escolhido.

Nada aqui pede permissão nenhuma ao sistema: é o próprio diretório de dados do
aplicativo. Instalar é escrever ali; desinstalar é apagar.
"""
import hashlib
import os
import shutil
import ssl
import urllib.request

import deb

# Onde ficam os certificados extras que alguns downloads exigem.
CA_DIR = "/app/share/adv-br-ui/ca"

AGENTE = "adv-br"


def raiz():
    base = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    return os.path.join(base, "componentes")


def diretorio(componente):
    return os.path.join(raiz(), componente.chave)


def instalado(componente):
    """Instalado quer dizer: todos os arquivos que ele promete estão lá.

    Conferir o diretório não bastaria — uma instalação interrompida no meio
    deixa um diretório que existe e não serve.
    """
    destino = diretorio(componente)
    esperados = list(componente.arquivos.values())
    if componente.lancador:
        esperados.append(os.path.join("bin", componente.chave))
    return all(os.path.exists(os.path.join(destino, alvo)) for alvo in esperados)


def _contexto(componente):
    """O contexto TLS do download.

    Quando o componente nomeia um certificado, ele é ACRESCENTADO às âncoras
    do sistema, não as substitui: o servidor da Softplan serve uma cadeia
    incompleta, e o que falta é o intermediário. Com ele, a verificação
    continua indo até uma raiz confiável. Desligar a verificação seria trocar
    um problema de configuração alheia por um buraco no nosso lado.
    """
    contexto = ssl.create_default_context()
    if componente.ca:
        contexto.load_verify_locations(cafile=os.path.join(CA_DIR, componente.ca))
    return contexto


def baixar(componente, progresso=None):
    """Baixa o pacote e devolve os bytes, conferindo o sha256.

    `progresso` recebe (recebido, total) e serve para a barra da interface. O
    total pode vir zero quando o servidor não informa o tamanho.
    """
    pedido = urllib.request.Request(componente.url, headers={"User-Agent": AGENTE})
    partes = []
    recebido = 0
    with urllib.request.urlopen(pedido, timeout=60,
                                context=_contexto(componente)) as resposta:
        total = int(resposta.headers.get("Content-Length") or 0)
        while True:
            pedaco = resposta.read(64 * 1024)
            if not pedaco:
                break
            partes.append(pedaco)
            recebido += len(pedaco)
            if progresso:
                progresso(recebido, total)

    dados = b"".join(partes)
    digest = hashlib.sha256(dados).hexdigest()
    if digest != componente.sha256:
        # Um pacote diferente do conferido não é instalado, e a mensagem diz o
        # que se viu: pode ser o fabricante tendo publicado uma versão nova, e
        # aí o catálogo é que precisa mudar.
        raise ValueError(
            "o arquivo baixado não confere com o esperado.\n"
            "esperado: %s\nrecebido: %s" % (componente.sha256, digest))
    return dados


def _escrever_lancador(componente, destino):
    """Cria bin/<chave> para o componente que traz aplicativo com janela.

    O corpo vem do catálogo, e não de código por componente: acrescentar um
    aplicativo novo continua sendo uma entrada lá e nada mais.
    """
    if not componente.lancador:
        return
    caminho = os.path.join(destino, "bin", componente.chave)
    os.makedirs(os.path.dirname(caminho), exist_ok=True)
    with open(caminho, "w", encoding="utf-8") as arquivo:
        arquivo.write("#!/bin/sh\nset -eu\n"
                      'COMPONENTE=$(cd -- "$(dirname -- "$0")/.." && pwd)\n'
                      + componente.lancador)
    os.chmod(caminho, 0o755)


def instalar(componente, progresso=None):
    dados = baixar(componente, progresso)
    destino = diretorio(componente)

    # Instala num diretório ao lado e só então troca: uma queda no meio da
    # extração não pode deixar meio driver instalado, que é pior que nenhum.
    temporario = destino + ".parcial"
    shutil.rmtree(temporario, ignore_errors=True)
    try:
        deb.extrair(dados, temporario, componente.arquivos)
        _escrever_lancador(componente, temporario)
        shutil.rmtree(destino, ignore_errors=True)
        os.replace(temporario, destino)
    finally:
        shutil.rmtree(temporario, ignore_errors=True)
    return destino


def desinstalar(componente):
    shutil.rmtree(diretorio(componente), ignore_errors=True)
