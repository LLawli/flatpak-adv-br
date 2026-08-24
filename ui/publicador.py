"""Leva o token do aplicativo para os programas que já existem na máquina.

São dois problemas diferentes, e por isso duas coisas escritas:

  autenticação por certificado   o navegador carrega o módulo PKCS#11 dentro do
  (Projudi, eproc, gov.br)       próprio processo. Escrevemos um .module que
                                 manda o p11-kit iniciar este aplicativo e
                                 conversar com ele por um pipe, e registramos o
                                 módulo no banco NSS de cada perfil.

  assinatura                     quem fala com o token é um programa à parte,
  (SAJ, portal da OAB)           que o navegador executa e com quem conversa
                                 por stdin/stdout. Escrevemos o manifesto de
                                 native messaging apontando para um atalho que
                                 entra neste aplicativo.

Nada é instalado no sistema: o que se escreve são arquivos de configuração
dentro da própria home de quem usa, e a remoção apaga exatamente esses.
"""
import glob
import json
import os

import catalogo
import instalador
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


def _id_do_flatpak(caminho):
    """O id do aplicativo a partir de um caminho dentro de ~/.var/app."""
    resto = caminho[len(os.path.join(os.path.expanduser("~"), ".var", "app")) + 1:]
    return resto.split(os.sep)[0]


def _e_de_flatpak(banco):
    return os.path.join(os.path.expanduser("~"), ".var", "app") in banco


def publicar():
    """Escreve o que os navegadores precisam. Devolve o que foi feito."""
    # sandbox_*: ids dos navegadores em Flatpak alcançados, separados pelo que
    # cada caso exige do lado deles. Ver permissoes.comandos_de_navegador: o
    # aplicativo não pode conceder essas permissões, e sem elas o navegador em
    # sandbox não enxerga nada do que foi publicado aqui.
    feito = {"modulos": [], "bancos": [], "erros": [],
             "sandbox_client": set(), "sandbox_assinador": set()}

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

    try:
        _publicar_assinadores(feito)
    except OSError as erro:
        feito["erros"].append("assinadores: %s" % erro)

    for banco in nssdb.bancos(_raizes_de_banco()):
        try:
            if _e_de_flatpak(banco):
                # Banco privado de um Flatpak: quem o abre é o programa de
                # dentro, e lá o caminho que existe é o do client.so do runtime.
                mudou = nssdb.registrar(banco, nssdb.NOME_SANDBOX,
                                        nssdb.CLIENT_NO_SANDBOX)
                # Entra na lista mesmo quando nada mudou: a permissão continua
                # sendo necessária, e "já estava registrado" é justamente o
                # caso de quem publicou antes e nunca soube que faltava algo.
                feito["sandbox_client"].add(_id_do_flatpak(banco))
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
    # sandbox_*: ids dos navegadores em Flatpak alcançados, separados pelo que
    # cada caso exige do lado deles. Ver permissoes.comandos_de_navegador: o
    # aplicativo não pode conceder essas permissões, e sem elas o navegador em
    # sandbox não enxerga nada do que foi publicado aqui.
    feito = {"modulos": [], "bancos": [], "erros": [],
             "sandbox_client": set(), "sandbox_assinador": set()}
    for arquivo in glob.glob(os.path.join(modulos_do_host(), PREFIXO + "*.module")):
        try:
            os.unlink(arquivo)
            feito["modulos"].append(os.path.basename(arquivo))
        except OSError as erro:
            feito["erros"].append(str(erro))

    # Manifestos e atalhos dos assinadores: o mesmo laço da publicação, com a
    # lista de vivos vazia.
    for casa, id_flatpak in _casas():
        for relativo, _ in NAVEGADORES:
            for arquivo in glob.glob(os.path.join(casa, relativo, "*.json")):
                try:
                    with open(arquivo, encoding="utf-8") as f:
                        if PREFIXO not in f.read():
                            continue
                    os.unlink(arquivo)
                    feito.setdefault("assinadores", []).append(
                        os.path.basename(arquivo))
                except OSError as erro:
                    feito["erros"].append(str(erro))

        pasta = (os.path.join(casa, SUBDIR_FLATPAK) if id_flatpak
                 else os.path.join(casa, ".local", "bin"))
        for arquivo in glob.glob(os.path.join(pasta, PREFIXO + "*")):
            try:
                os.unlink(arquivo)
            except OSError as erro:
                feito["erros"].append(str(erro))
        # O diretório que os continha é nosso; vazio, ele não deveria ficar.
        if id_flatpak and os.path.isdir(pasta) and not os.listdir(pasta):
            os.rmdir(pasta)

    for banco in nssdb.bancos(_raizes_de_banco()):
        for nome in (nssdb.NOME_HOST, nssdb.NOME_SANDBOX):
            try:
                if nssdb.remover(banco, nome):
                    feito["bancos"].append(banco)
            except OSError as erro:
                feito["erros"].append("banco %s: %s" % (banco, erro))
    return feito


# Onde cada família de navegador procura manifesto de native messaging, e qual
# campo cada uma exige. Trocar os campos é um erro mudo: o navegador ignora o
# arquivo sem dizer nada, e a extensão informa que o assinador não está
# instalado, que é o mesmo sintoma de nunca ter sido publicado.
#
#   diretório relativo à casa indicada  |  família
NAVEGADORES = [
    (".mozilla/native-messaging-hosts", "firefox"),
    (".config/google-chrome/NativeMessagingHosts", "chromium"),
    (".config/chromium/NativeMessagingHosts", "chromium"),
    (".config/BraveSoftware/Brave-Browser/NativeMessagingHosts", "chromium"),
    (".config/vivaldi/NativeMessagingHosts", "chromium"),
    (".config/microsoft-edge/NativeMessagingHosts", "chromium"),
    (".config/opera/NativeMessagingHosts", "chromium"),
]

# Onde o atalho de um navegador em Flatpak precisa morar.
#
# Não é .local/bin: um sandbox não enxerga isso. O Flatpak monta de
# ~/.var/app/<id> apenas os diretórios XDG e o que o aplicativo declarar como
# persistente, e o Firefox declara só o .mozilla. "data" tem a propriedade que
# resolve o resto: o caminho absoluto é o mesmo dentro e fora do sandbox, então
# o que se grava no manifesto vale dos dois lados.
SUBDIR_FLATPAK = "data/adv-br"


def _casas():
    """(casa, é_flatpak) de cada lugar onde procurar navegador."""
    casa = os.path.expanduser("~")
    lugares = [(casa, None)]
    for app in sorted(glob.glob(os.path.join(casa, ".var", "app", "*"))):
        lugares.append((app, os.path.basename(app)))
    return lugares


def _atalho(chave, casa, id_flatpak):
    """Escreve o atalho que o navegador executa, e devolve o caminho dele."""
    if id_flatpak:
        destino = os.path.join(casa, SUBDIR_FLATPAK)
        # De dentro de um sandbox não existe "flatpak", só o portal.
        comando = ("exec flatpak-spawn --host flatpak run "
                   "--command=adv-br-assinador %s %s \"$@\"\n" % (APP_ID, chave))
    else:
        destino = os.path.join(casa, ".local", "bin")
        comando = ("exec flatpak run --command=adv-br-assinador %s %s \"$@\"\n"
                   % (APP_ID, chave))

    os.makedirs(destino, exist_ok=True)
    caminho = os.path.join(destino, PREFIXO + chave)
    with open(caminho, "w", encoding="utf-8") as arquivo:
        arquivo.write("#!/bin/sh\n# Escrito pelo %s.\n%s" % (APP_ID, comando))
    os.chmod(caminho, 0o755)
    return caminho


def _publicar_assinadores(feito):
    instalados = [c for c in catalogo.por_tipo("assinador")
                  if instalador.instalado(c)]

    vivos = set()
    for casa, id_flatpak in _casas():
        for relativo, familia in NAVEGADORES:
            destino = os.path.join(casa, relativo)
            # O diretório-pai é o que diz se este navegador existe aqui: criar
            # ~/.config/vivaldi numa máquina sem Vivaldi seria inventar
            # navegador.
            if not os.path.isdir(os.path.dirname(destino)):
                continue

            for componente in instalados:
                origem = os.path.join(instalador.diretorio(componente),
                                      "native-messaging")
                for arquivo in sorted(glob.glob(os.path.join(origem, "*.%s.json" % familia))):
                    nome = os.path.basename(arquivo).rsplit(".", 2)[0]
                    try:
                        with open(arquivo, encoding="utf-8") as f:
                            manifesto = json.load(f)
                    except (OSError, ValueError) as erro:
                        feito["erros"].append("%s: %s" % (arquivo, erro))
                        continue

                    manifesto["path"] = _atalho(componente.chave, casa, id_flatpak)
                    if id_flatpak:
                        feito["sandbox_assinador"].add(id_flatpak)
                    os.makedirs(destino, exist_ok=True)
                    alvo = os.path.join(destino, nome + ".json")
                    with open(alvo, "w", encoding="utf-8") as f:
                        json.dump(manifesto, f, ensure_ascii=False, indent=2)
                        f.write("\n")
                    feito.setdefault("assinadores", []).append(nome)
                    vivos.add(alvo)

            # O manifesto de um assinador que saiu aponta para um atalho que já
            # não existe, e o navegador diz que ele não está instalado.
            for arquivo in glob.glob(os.path.join(destino, "*.json")):
                if arquivo in vivos:
                    continue
                try:
                    with open(arquivo, encoding="utf-8") as f:
                        if PREFIXO not in f.read():
                            continue
                except OSError:
                    continue
                os.unlink(arquivo)

    # Atalhos órfãos, pelo mesmo motivo.
    chaves_vivas = {PREFIXO + c.chave for c in instalados}
    for casa, id_flatpak in _casas():
        pasta = (os.path.join(casa, SUBDIR_FLATPAK) if id_flatpak
                 else os.path.join(casa, ".local", "bin"))
        for arquivo in glob.glob(os.path.join(pasta, PREFIXO + "*")):
            if os.path.basename(arquivo) not in chaves_vivas:
                os.unlink(arquivo)


def publicado():
    """Está publicado quando há pelo menos um .module nosso no host."""
    return bool(glob.glob(os.path.join(modulos_do_host(), PREFIXO + "*.module")))
