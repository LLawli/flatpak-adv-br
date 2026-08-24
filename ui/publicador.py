"""Leva o token do aplicativo para os programas que já existem na máquina.

São dois problemas diferentes, e por isso duas coisas escritas:

  autenticação por certificado   o navegador carrega o módulo PKCS#11 dentro do
  (Projudi, eproc, gov.br)       próprio processo. Escrevemos um .module que
                                 manda o p11-kit iniciar este aplicativo e
                                 conversar com ele por um pipe, e registramos o
                                 módulo no banco NSS de cada perfil.

  assinatura                     quem fala com o token é um programa à parte.
  (SAJ, portal da OAB)           Ainda não implementado aqui: entra junto com
                                 os assinadores no catálogo.

Nada é instalado no sistema: o que se escreve são arquivos de configuração
dentro da própria home de quem usa, e a remoção apaga exatamente esses.
"""
import glob
import os

import nssdb
import pkcs11

APP_ID = "dev.lukakuuhaku.AdvBr"

# O prefixo com que este aplicativo marca o que é dele.
#
# É "advbr-" e não "adv-br-" de propósito: a versão de linha de comando usa o
# segundo, e as duas convivem na mesma máquina enquanto os testadores exercitam
# aquela. Prefixos distintos fazem cada uma remover só o que escreveu; sem
# isso, publicar por aqui apagaria a publicação de lá sem avisar.
PREFIXO = "advbr-"


def _config():
    return os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")


def modulos_do_host():
    return os.path.join(_config(), "pkcs11", "modules")


def _raizes_de_banco():
    """Onde procurar banco NSS: os do host e os dos navegadores em Flatpak.

    O Firefox 147 moveu o perfil para XDG_CONFIG_HOME e deixou o resto onde
    estava, então os dois caminhos entram.
    """
    casa = os.path.expanduser("~")
    raizes = [
        os.path.join(casa, ".pki", "nssdb"),
        os.path.join(casa, ".mozilla", "firefox"),
        os.path.join(_config(), "mozilla", "firefox"),
    ]
    for app in sorted(glob.glob(os.path.join(casa, ".var", "app", "*"))):
        raizes += [
            os.path.join(app, ".pki", "nssdb"),
            os.path.join(app, ".mozilla", "firefox"),
            os.path.join(app, "config", "mozilla", "firefox"),
        ]
    return raizes


def _e_de_flatpak(banco):
    return os.path.join(os.path.expanduser("~"), ".var", "app") in banco


def publicar():
    """Escreve o que os navegadores precisam. Devolve o que foi feito."""
    feito = {"modulos": [], "bancos": [], "erros": []}

    destino = modulos_do_host()
    try:
        os.makedirs(destino, exist_ok=True)
    except OSError as erro:
        feito["erros"].append("não consegui criar %s: %s" % (destino, erro))
        return feito

    # Um .module por driver, e não um só para todos: um driver que derrube o
    # processo que o carregou leva junto apenas a si mesmo. E cada um vira um
    # processo separado, iniciado sob demanda pelo p11-kit do host.
    vivos = set()
    for caminho in pkcs11.modulos_instalados():
        nome = PREFIXO + os.path.splitext(os.path.basename(caminho))[0]
        arquivo = os.path.join(destino, nome + ".module")
        try:
            with open(arquivo, "w", encoding="utf-8") as f:
                f.write(
                    "# Escrito pelo %s.\n"
                    "# O p11-kit inicia este comando sob demanda e conversa com ele\n"
                    "# pelo pipe; do outro lado está o driver, dentro do Flatpak.\n"
                    "remote: |flatpak run --command=adv-br-pkcs11 %s %s\n"
                    % (APP_ID, APP_ID, caminho))
            feito["modulos"].append(nome)
            vivos.add(nome + ".module")
        except OSError as erro:
            feito["erros"].append("não consegui escrever %s: %s" % (arquivo, erro))

    # Um .module de driver que já não existe faria o p11-kit tentar abri-lo a
    # cada abertura de navegador, e falhar.
    for arquivo in glob.glob(os.path.join(destino, PREFIXO + "*.module")):
        if os.path.basename(arquivo) not in vivos:
            os.unlink(arquivo)

    proxy = pkcs11.proxy_do_host()
    if not proxy:
        feito["erros"].append(
            "não encontrei o p11-kit do sistema; os navegadores que rodam fora "
            "de sandbox podem não enxergar o token.")

    for banco in nssdb.bancos(_raizes_de_banco()):
        try:
            if _e_de_flatpak(banco):
                # Banco privado de um Flatpak: quem o abre é o programa de
                # dentro, e lá o caminho que existe é o do client.so do runtime.
                mudou = nssdb.registrar(banco, nssdb.NOME_SANDBOX,
                                        nssdb.CLIENT_NO_SANDBOX)
            else:
                # Um banco no home real é lido dos dois lados: pelo programa do
                # host, que carrega o proxy, e por um Flatpak com acesso ao
                # home, como o Papers. Os dois registros convivem porque o NSS
                # ignora em silêncio o módulo que não conseguir carregar.
                mudou = False
                if proxy:
                    mudou = nssdb.registrar(banco, nssdb.NOME_HOST, proxy)
                mudou = nssdb.registrar(banco, nssdb.NOME_SANDBOX,
                                        nssdb.CLIENT_NO_SANDBOX) or mudou
            if mudou:
                feito["bancos"].append(banco)
        except OSError as erro:
            feito["erros"].append("banco %s: %s" % (banco, erro))

    return feito


def despublicar():
    feito = {"modulos": [], "bancos": [], "erros": []}
    for arquivo in glob.glob(os.path.join(modulos_do_host(), PREFIXO + "*.module")):
        try:
            os.unlink(arquivo)
            feito["modulos"].append(os.path.basename(arquivo))
        except OSError as erro:
            feito["erros"].append(str(erro))

    for banco in nssdb.bancos(_raizes_de_banco()):
        for nome in (nssdb.NOME_HOST, nssdb.NOME_SANDBOX):
            try:
                if nssdb.remover(banco, nome):
                    feito["bancos"].append(banco)
            except OSError as erro:
                feito["erros"].append("banco %s: %s" % (banco, erro))
    return feito


def publicado():
    """Está publicado quando há pelo menos um .module nosso no host."""
    return bool(glob.glob(os.path.join(modulos_do_host(), PREFIXO + "*.module")))
