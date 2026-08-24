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
    "chave nome resumo detalhe tipo url sha256 arquivos tamanho ca",
)
# `ca` nomeia um certificado extra a confiar no download, e existe por um
# servidor só; ver o Softplan abaixo. Vazio no resto.
Componente.__new__.__defaults__ = ("",)

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


CATALOGO += [
    Componente(
        chave="webpki",
        nome="Lacuna Web PKI",
        resumo="Assinar em sistemas que usam o componente da Lacuna",
        detalhe=(
            "Instale se o site pedir o Web PKI. Depois é preciso instalar "
            "também a extensão do Lacuna no seu navegador."
        ),
        tipo="assinador",
        url="https://get.webpkiplugin.com/Downloads/2.16.0/setup-deb-64",
        sha256="d98752344e050b7fb040df3fb224998ec466bbffb96ac5e96e1ce455adee0a49",
        arquivos={
            # O nome do executável é o que o resto do aplicativo procura, e é
            # o que aparece no manifesto de native messaging.
            "./opt/lacuna-webpki/webpki": "bin/webpki",
            # Os manifestos do fabricante, um por família de navegador. Quem
            # responde por "quais extensões podem falar com este assinador" é
            # ele; o publicador só troca o campo "path".
            "./usr/share/mozilla/native-messaging-hosts/com.lacunasoftware.webpki.json":
                "native-messaging/com.lacunasoftware.webpki.firefox.json",
            "./opt/lacuna-webpki/manifest.json":
                "native-messaging/com.lacunasoftware.webpki.chromium.json",
        },
        tamanho=47 * 1024 * 1024,
    ),
    Componente(
        chave="websigner",
        nome="Softplan WebSigner",
        resumo="Assinar nos sistemas SAJ",
        detalhe=(
            "Usado pelos tribunais que rodam o SAJ. Depois é preciso instalar "
            "também a extensão do WebSigner no seu navegador."
        ),
        tipo="assinador",
        url="https://websigner.softplan.com.br/Downloads/2.15.0/webpki-chrome-64-deb",
        sha256="04fa41e962d91e4d7337f4707479437bf660f19057fac63829fb46784ee08289",
        arquivos={
            "./opt/softplan-websigner/websigner": "bin/websigner",
            "./usr/share/mozilla/native-messaging-hosts/br.com.softplan.webpki.json":
                "native-messaging/br.com.softplan.webpki.firefox.json",
            "./opt/softplan-websigner/manifest.json":
                "native-messaging/br.com.softplan.webpki.chromium.json",
        },
        tamanho=47 * 1024 * 1024,
        # O servidor da Softplan serve uma cadeia incompleta: repete o
        # certificado dele no lugar do intermediário. Sem este arquivo, o
        # download falha com "unable to get local issuer certificate".
        #
        # A saída não é desligar a verificação, é COMPLETAR a cadeia: este é o
        # intermediário que falta, emitido por uma raiz que o sistema já
        # confia. Com ele, a validação continua indo até o fim.
        ca="ThawteTLSRSACAG1.pem",
    ),
    Componente(
        chave="certisign",
        nome="Certisign WebSigner",
        resumo="Assinar no portal da OAB",
        detalhe=(
            "Usado pelo portal de assinatura eletrônica da OAB. Depois é "
            "preciso instalar também a extensão do Certisign no seu navegador."
        ),
        tipo="assinador",
        url="https://get.websignerplugin.com/Downloads/2.17.7/setup-deb-64",
        sha256="04981f073f61ac7e8662ec12f3d69be1cb8090131836935a111ef9d5b012abfb",
        arquivos={
            "./opt/certisign-websigner/cswebsigner": "bin/certisign",
            # A interface dele abre estes arquivos por caminho absoluto em
            # /opt; o lançador acomoda isso com um link na raiz do sandbox,
            # que é um tmpfs gravável.
            "./opt/certisign-websigner/res/": "res",
            "./usr/share/mozilla/native-messaging-hosts/br.com.certisign.websigner.json":
                "native-messaging/br.com.certisign.websigner.firefox.json",
            "./opt/certisign-websigner/manifest.json":
                "native-messaging/br.com.certisign.websigner.chromium.json",
        },
        tamanho=1 * 1024 * 1024,
    ),
]

POR_CHAVE = {c.chave: c for c in CATALOGO}


def por_tipo(tipo):
    return [c for c in CATALOGO if c.tipo == tipo]
