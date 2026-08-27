"""Baixa, confere e instala um componente nos dados do aplicativo.

Onde: $XDG_DATA_HOME/componentes/<Chave>/, que dentro do sandbox é gravável e
de onde um .so carrega. Verificado antes de o desenho ser escolhido.

Nada aqui pede permissão nenhuma ao sistema: é o próprio diretório de dados do
aplicativo. Instalar é escrever ali; desinstalar é apagar.
"""
import hashlib
import io
import os
import shutil
import ssl
import urllib.request
import zipfile

import catalogo
import deb
import registro

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
    for fonte in componente.fontes:
        esperados += list(fonte.arquivos.values())
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


def _fontes(componente):
    """As fontes do componente, seja ele de uma só ou de várias."""
    if componente.fontes:
        return list(componente.fontes)
    return [catalogo.Fonte(url=componente.url, sha256=componente.sha256,
                           arquivos=componente.arquivos,
                           formato="zip" if componente.dentro_de_zip else "deb")]


# De onde vêm os componentes do próprio projeto (hoje só o p11-kit de
# compatibilidade). Trocável por variável de ambiente para poder exercitar a
# instalação contra um servidor local, sem depender do que está publicado.
ORIGEM = "https://flatpak.lukakuuhaku.dev"


def _url(fonte):
    origem = os.environ.get("ADV_BR_ORIGEM")
    if origem and fonte.url.startswith(ORIGEM):
        return origem + fonte.url[len(ORIGEM):]
    return fonte.url


def baixar(componente, progresso=None, fonte=None):
    """Baixa um pacote e devolve os bytes, conferindo o sha256.

    `progresso` recebe (recebido, total) e serve para a barra da interface. O
    total pode vir zero quando o servidor não informa o tamanho.
    """
    fonte = fonte or _fontes(componente)[0]
    pedido = urllib.request.Request(_url(fonte), headers={"User-Agent": AGENTE})
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
    if digest != fonte.sha256:
        # Um pacote diferente do conferido não é instalado, e a mensagem diz o
        # que se viu: pode ser o fabricante tendo publicado uma versão nova, e
        # aí o catálogo é que precisa mudar.
        raise ValueError(
            "o arquivo baixado não confere com o esperado.\n"
            "esperado: %s\nrecebido: %s" % (fonte.sha256, digest))
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
                      # O aplicativo do componente é lançado pela janela e
                      # herdaria o stderr dela, misturando o log do SerproID
                      # com o do aplicativo. Um arquivo por componente.
                      'ADV_BR_MODULO=app-%s\n'
                      ". /app/share/adv-br-ui/registro.sh\n"
                      'COMPONENTE=$(cd -- "$(dirname -- "$0")/.." && pwd)\n'
                      % componente.chave
                      + componente.lancador)
    os.chmod(caminho, 0o755)


def _aplicar_trocas(componente, destino):
    """Reescreve as linhas que o catálogo manda trocar, no que foi extraído.

    Uma linha só, e por prefixo: é o suficiente para desligar o atualizador
    automático do PJeOffice, e qualquer coisa mais esperta seria editar o
    arquivo de outro projeto às cegas.
    """
    for relativo, (prefixo, nova) in componente.trocas.items():
        caminho = os.path.join(destino, relativo)
        with open(caminho, encoding="utf-8") as arquivo:
            linhas = arquivo.readlines()
        trocou = False
        for i, linha in enumerate(linhas):
            if linha.strip().startswith(prefixo):
                linhas[i] = nova + "\n"
                trocou = True
        if not trocou:
            # Silêncio aqui seria pior: o pacote mudou de forma e o que a
            # troca evitava (o programa se atualizar sozinho por cima do que
            # se instalou) volta a acontecer, sem aviso.
            raise ValueError("%s não tem nenhuma linha começando por %s"
                             % (relativo, prefixo))
        with open(caminho, "w", encoding="utf-8") as arquivo:
            arquivo.writelines(linhas)


def instalar(componente, progresso=None):
    destino = diretorio(componente)

    # Instala num diretório ao lado e só então troca: uma queda no meio da
    # extração não pode deixar meio driver instalado, que é pior que nenhum.
    temporario = destino + ".parcial"
    shutil.rmtree(temporario, ignore_errors=True)

    fontes = _fontes(componente)
    try:
        for numero, fonte in enumerate(fontes):
            # A barra anda de 0 a 100 dentro de cada pacote; com mais de um, a
            # fatia de cada um é proporcional. Sem isso a barra completaria e
            # recomeçaria, que é o jeito mais rápido de alguém achar que travou.
            def parcial(recebido, total, numero=numero):
                if not progresso:
                    return
                fatia = 1.0 / len(fontes)
                andado = (recebido / total if total else 0.0) * fatia
                progresso(int((numero * fatia + andado) * 1000), 1000)

            dados = baixar(componente, parcial, fonte)

            if componente.dentro_de_zip:
                # Alguns fabricantes distribuem um zip com instaladores para
                # várias distribuições. O sha256 conferido é o do zip, que é o
                # que se baixou.
                with zipfile.ZipFile(io.BytesIO(dados)) as pacote:
                    try:
                        dados = pacote.read(componente.dentro_de_zip)
                    except KeyError as erro:
                        raise ValueError(
                            "o arquivo baixado não traz %s. O fabricante pode "
                            "ter mudado o conteúdo do pacote."
                            % componente.dentro_de_zip) from erro

            if fonte.formato == "tar":
                deb.extrair_tar(dados, temporario, fonte.arquivos, fonte.cortar)
            else:
                deb.extrair(dados, temporario, fonte.arquivos)

        _aplicar_trocas(componente, temporario)
        _escrever_lancador(componente, temporario)
        shutil.rmtree(destino, ignore_errors=True)
        os.replace(temporario, destino)
    finally:
        shutil.rmtree(temporario, ignore_errors=True)
    _escrever_atalho(componente)
    return destino


# O atalho vai para o menu do HOST, e não para os dados do aplicativo: é ali que
# o lançador de aplicativos procura. Dentro do sandbox, ~ é o home de verdade e
# este diretório chega montado pelo --filesystem=xdg-data/applications:create.
ATALHOS = os.path.expanduser("~/.local/share/applications")


def _atalho(componente):
    return os.path.join(ATALHOS, "%s.%s.desktop" % (catalogo.APP_ID, componente.chave))


def _escrever_atalho(componente):
    """Põe no menu o componente que traz aplicativo com janela.

    Sem isto, abrir o PJeOffice custa abrir o Certificado Digital, rolar até o
    fim da lista e clicar em abrir. Um usuário pediu, e a razão é boa: o
    assinador é usado sozinho, várias vezes por dia, e não faz parte do fluxo de
    instalar componente.

    O Exec entra pelo adv-br-aplicativo, e não pelo lançador do componente, para
    o preparo dos drivers acontecer também aqui. Ver ui/adv-br-aplicativo.
    """
    if not componente.lancador:
        return
    try:
        os.makedirs(ATALHOS, exist_ok=True)
        with open(_atalho(componente), "w", encoding="utf-8") as arquivo:
            arquivo.write(
                "[Desktop Entry]\n"
                "Type=Application\n"
                "Name=%s\n"
                "Comment=%s\n"
                "Exec=flatpak run --command=adv-br-aplicativo %s %s\n"
                "Icon=%s\n"
                "Terminal=false\n"
                "StartupNotify=true\n"
                "Categories=Office;\n"
                "X-Flatpak=%s\n"
                % (componente.nome, componente.resumo,
                   catalogo.APP_ID, componente.chave,
                   catalogo.APP_ID, catalogo.APP_ID))
    except OSError as erro:
        # Não é motivo para a instalação falhar: o componente está instalado e
        # abre pela janela do mesmo jeito.
        registro.falha("não consegui criar o atalho de %s" % componente.chave, erro)


def _remover_atalho(componente):
    try:
        os.remove(_atalho(componente))
    except FileNotFoundError:
        pass
    except OSError as erro:
        registro.falha("não consegui remover o atalho de %s" % componente.chave, erro)


def lancador(componente):
    """Caminho do executável do componente, quando ele traz um aplicativo."""
    if not componente.lancador:
        return ""
    caminho = os.path.join(diretorio(componente), "bin", componente.chave)
    return caminho if os.access(caminho, os.X_OK) else ""


def desinstalar(componente):
    _remover_atalho(componente)
    shutil.rmtree(diretorio(componente), ignore_errors=True)
