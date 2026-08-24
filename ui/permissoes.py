"""O que o aplicativo precisa alcançar fora do sandbox, e o que fazer quando não alcança.

Publicar o token para os programas que já existem na máquina é escrever
configuração na home de quem usa. Isso é a única coisa aqui que depende de
permissão, e ela pode faltar: alguém revogou, uma política da distribuição
apertou, ou a loja entregou o aplicativo com menos do que o manifesto pede.

Falhar em silêncio nesse caso seria o pior resultado, porque o sintoma
apareceria longe: o token não aparece no navegador, e nada explica por quê. Daí
este módulo, que responde duas perguntas: o que falta, e qual comando devolve.
"""
import os

APP_ID = "dev.lukakuuhaku.AdvBr"


def _config():
    return os.environ.get("XDG_CONFIG_HOME") or os.path.expanduser("~/.config")


def _dados():
    return os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")


def _casa():
    return os.path.expanduser("~")


# O que precisa estar montado  →  para que serve  →  o que devolve a permissão
#
# O primeiro campo é o PONTO EXATO que o Flatpak monta, e não um caminho
# qualquer dentro dele. O Flatpak monta cada --filesystem em dois lugares (o
# caminho do host e o correspondente dentro do diretório XDG do aplicativo), e
# qualquer um dos dois serve de prova.
NECESSARIOS = [
    ([os.path.join(_config(), "pkcs11"),
      os.path.join(_casa(), ".config", "pkcs11")],
     "levar o token aos programas do sistema",
     "--filesystem=xdg-config/pkcs11:create"),
    ([os.path.join(_casa(), ".pki")],
     "o banco de certificados do Chrome, do Brave e do Papers",
     "--filesystem=~/.pki:create"),
    ([os.path.join(_casa(), ".mozilla")],
     "o Firefox instalado no sistema",
     "--filesystem=~/.mozilla:create"),
    ([os.path.join(_dados(), "applications"),
      os.path.join(_casa(), ".local", "share", "applications")],
     "os atalhos no menu de aplicativos",
     "--filesystem=xdg-data/applications:create"),
    ([os.path.join(_casa(), ".local", "bin")],
     "os atalhos que o navegador executa para assinar",
     "--filesystem=~/.local/bin:create"),
]


def _pontos_de_montagem():
    """Os caminhos que o Flatpak montou dentro deste sandbox."""
    pontos = set()
    try:
        with open("/proc/self/mountinfo", encoding="utf-8") as arquivo:
            for linha in arquivo:
                campos = linha.split(" ")
                if len(campos) > 4:
                    # O campo 5 é o ponto de montagem, com octais escapados.
                    pontos.add(campos[4].replace("\\040", " "))
    except OSError:
        pass
    return pontos


def _alcanca(caminhos, pontos):
    """O sandbox alcança algum destes caminhos de verdade?

    Escrever nele NÃO responde a pergunta, e essa é a parte que engana: quando
    a permissão falta, o Flatpak não monta nada ali, o caminho passa a existir
    dentro do tmpfs do próprio sandbox, e criar arquivo funciona sem que nada
    chegue ao host. O teste ingênuo passa e o usuário fica sem entender por que
    o navegador não vê o certificado.

    Subir pelos ancestrais também não serve, e por um motivo específico deste
    aplicativo: ele pede --filesystem=~/.var/app para alcançar os navegadores
    em Flatpak, e esse caminho é ancestral do diretório XDG dele. Qualquer
    busca para cima acaba encontrando um ponto montado e respondendo "sim" para
    tudo. O que vale é o ponto exato.
    """
    return any(caminho in pontos for caminho in caminhos)


def faltando():
    """Lista de (caminhos, para que serve, argumento) que o sandbox não alcança."""
    pontos = _pontos_de_montagem()
    return [(caminhos, para_que, argumento)
            for caminhos, para_que, argumento in NECESSARIOS
            if not _alcanca(caminhos, pontos)]


def tem_documentos():
    """A pasta Documentos está montada neste sandbox?

    É a única permissão OPCIONAL do aplicativo: nada do que ele faz por conta
    própria precisa dela. Quem pede é o assinador de arquivos avulsos do
    PJeOffice, e por isso ela é oferecida depois de instalar, com o motivo, em
    vez de vir no manifesto. Permissão que o aplicativo não usa é permissão que
    ele não deve ter.
    """
    documentos = [os.path.join(_casa(), "Documentos"),
                  os.path.join(_casa(), "Documents"),
                  os.environ.get("XDG_DOCUMENTS_DIR", "")]
    return _alcanca([d for d in documentos if d], _pontos_de_montagem())


def comando_opcional(argumento):
    """O comando que concede UMA permissão opcional, pronto para colar."""
    return "flatpak override --user %s %s" % (argumento, APP_ID)


def comando(pendencias):
    """O comando que devolve as permissões que faltam, pronto para colar."""
    argumentos = " ".join(argumento for _, _, argumento in pendencias)
    return "flatpak override --user %s %s" % (argumentos, APP_ID)
