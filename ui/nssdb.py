"""Registra módulos PKCS#11 nos bancos NSS, editando o pkcs11.txt.

Sem `modutil`: num banco moderno (cert9.db + key4.db) a lista de módulos não
vive dentro do banco, vive num arquivo de texto ao lado dele. É esse arquivo
que o `modutil -add` edita, e é ele que este módulo edita.

Além de dispensar uma dependência que nem toda distribuição traz, isso resolve
de graça o caso que exigiria `modutil -rawadd`: registrar um caminho que só
existe dentro do sandbox de outro aplicativo.
"""
import glob
import os

# Os nomes com que este aplicativo marca o que registrou. O prefixo é o mesmo
# dos .module e existe pelo mesmo motivo: a versão de linha de comando usa
# "adv-br", e cada uma precisa remover só o que escreveu.
NOME_HOST = "advbr-proxy"
NOME_SANDBOX = "advbr-client"

# Onde o p11-kit-client.so mora dentro do runtime de um navegador em Flatpak.
# É caminho de lá, não daqui.
CLIENT_NO_SANDBOX = "/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so"


def _blocos(texto):
    return [b for b in (bloco.strip("\n") for bloco in texto.split("\n\n")) if b.strip()]


def _campo(bloco, chave):
    for linha in bloco.splitlines():
        if linha.startswith(chave + "="):
            return linha[len(chave) + 1:]
    return None


def bancos(raizes):
    """Diretórios com cert9.db sob as raízes dadas.

    Procurar o arquivo, em vez de deduzir o layout, é o que sobrevive ao
    Firefox ter mudado o perfil de lugar e aos forks que usam outro diretório.
    """
    encontrados = []
    for raiz in raizes:
        if not os.path.isdir(raiz):
            continue
        for cert in glob.glob(os.path.join(raiz, "cert9.db")) + \
                glob.glob(os.path.join(raiz, "*", "cert9.db")):
            encontrados.append(os.path.dirname(cert))
    return sorted(set(encontrados))


def registrados(banco):
    arquivo = os.path.join(banco, "pkcs11.txt")
    try:
        with open(arquivo, encoding="utf-8") as f:
            texto = f.read()
    except OSError:
        return {}
    return {_campo(b, "name"): _campo(b, "library") for b in _blocos(texto)
            if _campo(b, "name")}


def registrar(banco, nome, biblioteca):
    """Acrescenta ou atualiza um módulo. Devolve True se o arquivo mudou."""
    arquivo = os.path.join(banco, "pkcs11.txt")
    try:
        with open(arquivo, encoding="utf-8") as f:
            blocos = _blocos(f.read())
    except OSError:
        # Sem pkcs11.txt não há banco: criar um do zero exigiria escrever
        # também o bloco do módulo interno do NSS, com parâmetros que dependem
        # do caminho. Um perfil que nunca foi aberto não tem o que registrar.
        return False

    novo = "library=%s\nname=%s\nNSS=trustOrder=100" % (biblioteca, nome)
    for i, bloco in enumerate(blocos):
        if _campo(bloco, "name") == nome:
            if bloco.strip() == novo:
                return False
            blocos[i] = novo
            break
    else:
        blocos.append(novo)

    return _escrever(arquivo, blocos)


def remover(banco, nome):
    arquivo = os.path.join(banco, "pkcs11.txt")
    try:
        with open(arquivo, encoding="utf-8") as f:
            blocos = _blocos(f.read())
    except OSError:
        return False
    restantes = [b for b in blocos if _campo(b, "name") != nome]
    if len(restantes) == len(blocos):
        return False
    return _escrever(arquivo, restantes)


def _escrever(arquivo, blocos):
    # O NSS relê o arquivo inteiro; escrever por cima de um aberto por um
    # navegador em execução é justamente o caso que não funciona, e é por isso
    # que a interface pede para fechar o navegador.
    temporario = arquivo + ".adv-br.tmp"
    with open(temporario, "w", encoding="utf-8") as f:
        f.write("\n\n".join(blocos) + "\n\n")
    os.replace(temporario, arquivo)
    return True
