"""O estado da máquina, em texto, para acompanhar um relato de erro.

É o irmão do diagnostico.sh da versão de linha de comando, com uma diferença de
propósito: aquele é para quem sabe ler um terminal, este é para ser anexado a
um relato por quem só quer assinar uma petição.

Tudo aqui é leitura. Nada é corrigido, nada é publicado, e o resultado passa
pela sanitização antes de ser mostrado a quem vai enviar.
"""
import glob
import os

import catalogo
import instalador
import permissoes
import pkcs11
import publicador
import registro
import sanitizar
import serie

# Quantas linhas do fim de cada log entram. O fim é o que interessa: é onde
# está o que aconteceu por último. Um relato com o log inteiro não é lido por
# ninguém, e o corpo de uma issue tem limite.
LINHAS_DE_LOG = 60


def versao():
    for caminho in ("/app/share/adv-br-ui/VERSAO",):
        try:
            with open(caminho, encoding="utf-8") as arquivo:
                return arquivo.read().strip()
        except OSError:
            continue
    return "desconhecida"


def _do_arquivo(caminho, campo):
    """Lê CAMPO=valor de um os-release, sem depender de nada instalado."""
    try:
        with open(caminho, encoding="utf-8") as arquivo:
            for linha in arquivo:
                if linha.startswith(campo + "="):
                    return linha.split("=", 1)[1].strip().strip('"')
    except OSError:
        return ""
    return ""


def _sistema():
    # O /usr do host está montado em /run/host; é de lá que sai o nome da
    # distribuição de quem usa, e não do runtime, que seria sempre o mesmo.
    for caminho in ("/run/host/os-release", "/run/host/usr/lib/os-release",
                    "/run/host/etc/os-release"):
        nome = _do_arquivo(caminho, "PRETTY_NAME")
        if nome:
            return nome
    return "não identificado"


def _logs():
    partes = []
    raiz = os.path.join(os.environ.get("XDG_DATA_HOME") or
                        os.path.expanduser("~/.local/share"), "logs")
    for caminho in sorted(glob.glob(os.path.join(raiz, "*.log"))):
        try:
            with open(caminho, encoding="utf-8", errors="replace") as arquivo:
                linhas = arquivo.readlines()
        except OSError as erro:
            partes.append("--- %s: não consegui ler (%s)" %
                          (os.path.basename(caminho), erro))
            continue
        cauda = linhas[-LINHAS_DE_LOG:]
        cortado = " (últimas %d de %d linhas)" % (len(cauda), len(linhas)) \
            if len(linhas) > len(cauda) else ""
        partes.append("--- %s%s\n%s" % (os.path.basename(caminho), cortado,
                                        "".join(cauda).rstrip()))
    if not partes:
        partes.append("(nenhum log ainda)")
    return "\n".join(partes)


def coletar():
    """O diagnóstico inteiro, já sanitizado, pronto para ser mostrado."""
    linhas = []
    escrever = linhas.append

    escrever("aplicativo: %s" % versao())
    escrever("sistema: %s" % _sistema())

    # As duas séries do p11-kit vêm primeiro entre os dados técnicos: quando
    # divergem, tudo o mais parece certo e nada assina.
    try:
        host, pacote = serie.do_host(), serie.do_pacote()
        escrever("p11-kit: host %s, pacote %s%s" % (
            host or "?", pacote or "?",
            "  <<< DIVERGEM" if host and pacote and host != pacote else ""))
    except Exception as erro:  # noqa: BLE001
        registro.falha("diagnóstico: séries do p11-kit", erro)
        escrever("p11-kit: não consegui comparar")

    instalados = [c.chave for c in catalogo.CATALOGO if instalador.instalado(c)]
    escrever("componentes: %s" % (", ".join(instalados) or "nenhum"))

    escrever("publicado: %s" % ("sim" if publicador.publicado() else "não"))
    faltando = [argumento for _, _, argumento in permissoes.faltando()]
    escrever("permissões faltando: %s" % (", ".join(faltando) or "nenhuma"))

    try:
        navegadores = []
        for casa, id_flatpak in publicador._casas():
            for caminho, familia in publicador.navegadores(casa):
                navegadores.append("%s%s" % (
                    familia, " (%s)" % id_flatpak if id_flatpak else ""))
        escrever("navegadores: %s" % (", ".join(navegadores) or "nenhum"))
    except Exception as erro:  # noqa: BLE001
        registro.falha("diagnóstico: navegadores", erro)

    try:
        tokens = pkcs11.tokens()
        escrever("tokens: %s" % (", ".join(
            "%s [%s]" % (t["rotulo"], t["modelo"]) for t in tokens) or "nenhum"))
        escrever("módulos: %s" % ", ".join(
            os.path.basename(m) for m in pkcs11.modulos_instalados()))
    except Exception as erro:  # noqa: BLE001
        registro.falha("diagnóstico: tokens", erro)
        escrever("tokens: erro ao ler")

    escrever("")
    escrever(_logs())

    # A sanitização é do texto inteiro, e não campo a campo: o rótulo do token
    # aparece aqui e também dentro dos logs.
    return sanitizar.sanitizar("\n".join(linhas))
