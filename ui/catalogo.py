"""O que o aplicativo sabe instalar, e de onde.

Cada componente é uma entrada com a URL do fabricante, o sha256 do que se
espera receber e o mapa do que extrair do pacote. Nada disto é redistribuído
pelo projeto: o download acontece na máquina de quem usa, e o sha256 é a
garantia de que o que chegou é o que foi conferido.

Um componente novo é uma entrada aqui e nada mais. Se precisar de código, a
convenção está errada.
"""
import collections

Componente = collections.namedtuple(
    "Componente",
    "chave nome resumo detalhe tipo url sha256 arquivos tamanho",
)

# tipo:
#   driver     apresenta o token ao sistema (vira módulo PKCS#11)
#   assinador  conversa com a extensão do navegador por native messaging
#   app        tem janela própria
CATALOGO = [
    Componente(
        chave="safesign",
        nome="SafeSign",
        resumo="Driver do token GD Burti",
        detalhe=(
            "O token mais usado na advocacia brasileira. Instale se o seu "
            "certificado não aparecer com o OpenSC, que já vem no aplicativo."
        ),
        tipo="driver",
        url=(
            "https://assets.ctfassets.net/zuadwp3l2xby/6vGICRnQgQ8TkcHTgcouIr/"
            "5acf96dcbc0364aa9228606d3969ef97/"
            "SafeSignICStandardLinux4.5.0.0-AET.000ub2404x86_64.deb"
        ),
        sha256="7742e21e3141e51e307d7613b4046886bc7c4aa203835dcf5c43cd348f2a1b91",
        # A biblioteca vem versionada; o nome sem versão é o que o resto do
        # aplicativo procura, e é o que fica gravado em qualquer configuração.
        arquivos={"./usr/lib/libaetpkss.so.3.9.33.1": "pkcs11/libaetpkss.so"},
        tamanho=10 * 1024 * 1024,
    ),
]

POR_CHAVE = {c.chave: c for c in CATALOGO}
