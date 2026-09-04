"""O socket do RemoteID atravessa a fronteira entre instâncias do sandbox.

O RemoteID é certificado em nuvem: o módulo PKCS#11 manda o digest para o
aplicativo por um socket UNIX, e é o aplicativo que pede o PIN e traz a
assinatura. Só que os dois nunca rodam na mesma instância deste Flatpak — quem
abre o módulo é a ponte do navegador, um assinador ou o PJeOffice, e o
aplicativo roda por si. O padrão do RemoteID é o $XDG_RUNTIME_DIR, que é
privado de cada instância: cada lado criaria o seu socket e nenhum acharia o
outro.

O sintoma disso é o pior que este projeto conhece: o certificado APARECE, o
navegador lista o token, e só a assinatura falha. Por isso a conferência é
aqui, e não na primeira vez que alguém for assinar.

São duas implementações da mesma decisão, uma por pacote (ui/preparar-drivers.sh
na versão com janela, src/comum-pkcs11.sh na de linha de comando). A prova mais
importante é a última: que as duas dizem a mesma coisa.
"""
import os
import shutil
import subprocess
import sys
import tempfile

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# As casas de mentira ficam FORA do /tmp, e não é preferência: o preparo liga
# /tmp/remoteid-teste, então este teste roda com um /tmp próprio (bwrap), e uma
# casa criada lá dentro sumiria junto com ele.
CASAS = os.path.join(os.environ.get("XDG_CACHE_HOME")
                     or os.path.expanduser("~/.cache"), "adv-br-testes")

# O que interessa saber de cada mundo, depois do preparo.
VARIAVEIS = ("REMOTEID_SOCKET", "REMOTEID_HOME", "REMOTEID_DIAG_DIR", "TEST_URL")

# Os dois preparos. O da janela é incluído direto; o da linha de comando define
# uma função e é preciso chamá-la.
MUNDOS = {
    "janela": ". %s/ui/preparar-drivers.sh" % RAIZ,
    "linha de comando": ". %s/src/comum-pkcs11.sh; preparar_remoteid" % RAIZ,
}


def preparar(mundo, casa, test_url=None):
    """Roda um dos preparos numa casa de mentira e devolve o que ele exportou.

    O /tmp vira um tmpfs próprio quando há bwrap, que é o caso em qualquer
    máquina com Flatpak: o preparo liga /tmp/remoteid-teste, e um teste não pode
    mexer no /tmp de quem o roda.
    """
    dados = os.path.join(casa, "data", "remoteid")
    os.makedirs(dados, exist_ok=True)
    if test_url is not None:
        with open(os.path.join(dados, "TEST_URL"), "w", encoding="utf-8") as f:
            f.write(test_url + "\n")

    corpo = "%s\nfor v in %s; do eval \"printf '%%s=%%s\\n' \\$v \\\"\\$$v\\\"\"; done" % (
        MUNDOS[mundo], " ".join(VARIAVEIS))
    ambiente = dict(os.environ,
                    HOME=casa,
                    XDG_DATA_HOME=os.path.join(casa, "data"),
                    XDG_CONFIG_HOME=os.path.join(casa, "config"))
    # As duas variáveis que o preparo exporta não podem vir de fora, ou o teste
    # estaria conferindo o próprio ambiente.
    for sobra in VARIAVEIS:
        ambiente.pop(sobra, None)

    comando = ["bash", "-c", corpo]
    if shutil.which("bwrap"):
        comando = ["bwrap", "--dev-bind", "/", "/", "--tmpfs", "/tmp"] + comando

    saida = subprocess.run(comando, env=ambiente, stdout=subprocess.PIPE,
                           stderr=subprocess.PIPE, check=False)
    exportado = {}
    for linha in saida.stdout.decode("utf-8", "replace").splitlines():
        nome, _, valor = linha.partition("=")
        if nome in VARIAVEIS:
            exportado[nome] = valor
    return exportado


# Os ids que o aplicativo do RemoteID registra no barramento de sessão. Estão
# escritos aqui, e não descobertos, porque vêm de outro repositório: é o mesmo
# tipo de acordo que o sha256 do catálogo, e quebra do mesmo jeito silencioso.
# Ver crates/remoteid-gtk/src/main.rs no RemoteID-linux.
NOMES_NO_BARRAMENTO = (
    "dev.lukakuuhaku.RemoteID",
    "dev.lukakuuhaku.RemoteID.Teste",
    "dev.lukakuuhaku.RemoteID.Preview",
)

MANIFESTOS = ("dev.lukakuuhaku.AdvBr.yml", "io.github.llawli.AdvBr.yml")


def _cobre(regra, nome):
    """Se um --own-name do Flatpak cobre este nome.

    A regra é a do próprio Flatpak: nome exato, ou um prefixo terminado em
    ".*", que casa os filhos e NÃO o pai.
    """
    if regra == nome:
        return True
    return regra.endswith(".*") and nome.startswith(regra[:-1])


def conferir_barramento():
    """Os manifestos liberam os nomes que o aplicativo do RemoteID registra.

    O filtro de barramento que o Flatpak monta por padrão só deixa o sandbox
    possuir nomes que comecem pelo id do PACOTE. O RemoteID chega como
    componente e registra um id próprio, então o pacote precisa dizer que
    permite. Sem isso ele morre no arranque com "Failed to register:
    ...ServiceUnknown" — uma mensagem que não fala em barramento, nem em nome,
    nem em permissão, e que foi exatamente como o defeito apareceu.
    """
    problemas = []
    for manifesto in MANIFESTOS:
        with open(manifesto, encoding="utf-8") as arquivo:
            regras = [linha.split("--own-name=", 1)[1].strip()
                      for linha in arquivo if "--own-name=" in linha]
        for nome in NOMES_NO_BARRAMENTO:
            if not any(_cobre(regra, nome) for regra in regras):
                problemas.append("%s: nenhum --own-name cobre %s" % (manifesto, nome))
    return problemas


def conferir():
    problemas = conferir_barramento()
    os.makedirs(CASAS, exist_ok=True)
    for mundo in MUNDOS:
        casa = tempfile.mkdtemp(prefix="prova-remoteid-", dir=CASAS)
        try:
            dados = os.path.join(casa, "data", "remoteid")

            # 1. Sem modo de teste: socket e estado nos dados do aplicativo, que
            #    é o único caminho que todas as instâncias enxergam.
            normal = preparar(mundo, casa)
            if normal.get("REMOTEID_SOCKET") != os.path.join(dados, "remoteid.sock"):
                problemas.append("%s: socket de produção em %r, e não nos dados"
                                 % (mundo, normal.get("REMOTEID_SOCKET")))
            if normal.get("REMOTEID_HOME") != os.path.join(dados, "estado"):
                problemas.append("%s: estado em %r, e não nos dados"
                                 % (mundo, normal.get("REMOTEID_HOME")))
            if normal.get("TEST_URL"):
                problemas.append("%s: ligou o modo de teste sem ninguém pedir" % mundo)

            # O RemoteID redige o que grava (senha, PIN e OTP nunca entram),
            # mas o diagnóstico dele IDENTIFICA o titular do certificado. O
            # "Relatar um problema" varre $XDG_DATA_HOME/logs e manda o que
            # achar, então o diagnóstico dele não pode morar lá dentro.
            logs = os.path.join(casa, "data", "logs")
            diag = normal.get("REMOTEID_DIAG_DIR") or ""
            if not diag:
                problemas.append("%s: não apontou REMOTEID_DIAG_DIR" % mundo)
            elif os.path.commonpath([os.path.realpath(diag) if os.path.exists(diag) else diag,
                                     logs]) == logs:
                problemas.append(
                    "%s: o diagnóstico do RemoteID (%r) cai na varredura do relator"
                    % (mundo, diag))

            # 2. Com modo de teste: a URL chega como variável, e o socket é
            #    OUTRO — um aplicativo em modo de teste não pode responder por
            #    um pedido de assinatura de verdade.
            teste = preparar(mundo, casa, "http://localhost:8799")
            if teste.get("TEST_URL") != "http://localhost:8799":
                problemas.append("%s: TEST_URL não chegou (%r)"
                                 % (mundo, teste.get("TEST_URL")))
            if teste.get("REMOTEID_SOCKET") != os.path.join(dados, "teste", "remoteid.sock"):
                problemas.append("%s: em teste, o socket é %r"
                                 % (mundo, teste.get("REMOTEID_SOCKET")))
            if teste.get("REMOTEID_SOCKET") == normal.get("REMOTEID_SOCKET"):
                problemas.append("%s: o socket de teste é o mesmo do de produção" % mundo)

            # 3. O arquivo é gravável por quem tiver acesso aos dados do
            #    aplicativo, e vira variável de ambiente de todo processo
            #    preparado. O que não for uma URL http(s) simples não passa.
            for recusar in ("http://x;rm -rf /", "file:///etc/passwd",
                            "http://x$(id)", "javascript:1"):
                sujo = preparar(mundo, casa, recusar)
                if sujo.get("TEST_URL"):
                    problemas.append("%s: aceitou %r como TEST_URL" % (mundo, recusar))
        finally:
            shutil.rmtree(casa, ignore_errors=True)

    # 4. E a que mais importa: os dois pacotes decidem igual. Divergir aqui é o
    #    tipo de defeito que só aparece em um dos dois, meses depois.
    casa_a = tempfile.mkdtemp(prefix="prova-remoteid-a-", dir=CASAS)
    casa_b = tempfile.mkdtemp(prefix="prova-remoteid-b-", dir=CASAS)
    try:
        for url in (None, "http://localhost:8799"):
            a = preparar("janela", casa_a, url)
            b = preparar("linha de comando", casa_b, url)
            relativo_a = {k: v.replace(casa_a, "") for k, v in a.items()}
            relativo_b = {k: v.replace(casa_b, "") for k, v in b.items()}
            if relativo_a != relativo_b:
                problemas.append(
                    "os dois pacotes divergem (TEST_URL=%r): %r contra %r"
                    % (url, relativo_a, relativo_b))
    finally:
        shutil.rmtree(casa_a, ignore_errors=True)
        shutil.rmtree(casa_b, ignore_errors=True)

    return problemas


def main():
    problemas = conferir()
    for problema in problemas:
        print("  " + problema, file=sys.stderr)
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
