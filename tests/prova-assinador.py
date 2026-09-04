"""Quem vai consumir módulo PKCS#11 precisa registrá-lo antes.

O p11-kit deste sandbox só conhece o que estiver em /etc/pkcs11/modules, e esse
diretório é um tmpfs recriado a cada execução: nenhum driver existe até que
alguém escreva os .module. Quem escreve é pkcs11.registrar().

A ponte PKCS#11 não precisa, porque recebe o caminho do módulo pronto. O
assinador e os aplicativos precisam, porque o que eles recebem é
/pkcs11/adv-br.so — um caminho que responde pelo p11-kit-proxy, e um proxy sem
registro não tem o que oferecer.

Foi um defeito de verdade, e o sintoma não parecia encanamento: a extensão do
navegador conectava, o getVersion respondia "2.16.0", e a lista de certificados
voltava vazia, como se o token não estivesse espetado. Medido: sem registrar, o
Lacuna Web PKI devolve 0 certificados; registrando, devolve os 2 que existem.
"""
import os
import re
import sys

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(RAIZ, "ui"))

import catalogo  # noqa: E402

REGISTRA = re.compile(r"pkcs11\.registrar\(\)")

# Os lançadores que entregam um consumidor de módulo, e o que cada um é.
LANCADORES = {
    "ui/adv-br-assinador": "o assinador que o navegador executa",
}


def _antes_do_exec(texto):
    """Se o registro acontece ANTES da linha que troca o processo.

    Depois do exec não existe "depois": aquele processo já é outro.
    """
    registro = REGISTRA.search(texto)
    if not registro:
        return False
    for achado in re.finditer(r"^\s*exec\s", texto, re.M):
        if achado.start() < registro.start():
            return False
    return True


def conferir():
    problemas = []

    for caminho, o_que_e in LANCADORES.items():
        with open(os.path.join(RAIZ, caminho), encoding="utf-8") as arquivo:
            texto = arquivo.read()
        if not REGISTRA.search(texto):
            problemas.append("%s (%s) não registra os módulos no p11-kit"
                             % (caminho, o_que_e))
        elif not _antes_do_exec(texto):
            problemas.append("%s registra depois de um exec, que nunca acontece"
                             % caminho)

    # E o aplicativo que chega como componente e LÊ token. Quem registra é o
    # corpo do lançador, no catálogo, e não o adv-br-aplicativo, então a
    # conferência é lá.
    #
    # A lista é explícita porque a distinção não está no catálogo e não dá para
    # deduzir: o SerproID e o RemoteID também trazem aplicativo, mas eles
    # SERVEM o token, não o leem — o do SerproID grava os .cer que a biblioteca
    # dele vai ler, e o do RemoteID atende o socket do módulo. Registrar não
    # faria nada por nenhum dos dois.
    for chave in ("pjeoffice",):
        componente = catalogo.POR_CHAVE.get(chave)
        if componente is None:
            problemas.append("o catálogo não tem mais o componente %s" % chave)
        elif not REGISTRA.search(componente.lancador):
            problemas.append(
                "o lançador de %s abre um aplicativo que lê token e não registra os módulos"
                % chave)

    return problemas


def main():
    problemas = conferir()
    for problema in problemas:
        print("  " + problema, file=sys.stderr)
    if not problemas:
        print("  ok  todo consumidor de módulo registra antes de subir")
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
