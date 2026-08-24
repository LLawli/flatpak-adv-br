#!/usr/bin/env python3
"""Registra e remove módulos PKCS#11 num banco NSS, editando o pkcs11.txt.

Existe para não depender do `modutil`. Num banco NSS moderno (cert9.db +
key4.db, o formato "sql:"), a lista de módulos não vive dentro do banco: vive
num arquivo de texto ao lado dele, `pkcs11.txt`. É esse arquivo que o
`modutil -add` edita, e é ele que este script edita.

A diferença que importa: o `modutil -add` carrega a biblioteca para validá-la,
e por isso não serve para registrar, a partir do host, um caminho que só existe
dentro do sandbox de um navegador em Flatpak — para esse caso ele exige
`-rawadd`. Aqui os dois casos são a mesma operação.

O formato do arquivo são blocos separados por linha em branco, cada linha um
`chave=valor`:

    library=/usr/lib64/p11-kit-proxy.so
    name=adv-br
    NSS=trustOrder=100

Uso:
    nssdb.py listar   <diretório-do-banco>
    nssdb.py registrar <diretório-do-banco> <nome> <caminho-da-biblioteca>
    nssdb.py remover  <diretório-do-banco> <nome>

Sai com 0 se algo mudou ou já estava como pedido, 1 em erro.
"""
import os
import sys


def blocos(texto):
    """Quebra o pkcs11.txt em blocos, preservando a ordem."""
    return [b for b in (bloco.strip("\n") for bloco in texto.split("\n\n"))
            if b.strip()]


def campo(bloco, chave):
    for linha in bloco.splitlines():
        if linha.startswith(chave + "="):
            return linha[len(chave) + 1:]
    return None


def ler(caminho):
    try:
        with open(caminho, encoding="utf-8") as f:
            return blocos(f.read())
    except FileNotFoundError:
        return []


def escrever(caminho, lista):
    # O NSS relê este arquivo inteiro; escrever por cima de um arquivo aberto
    # por um navegador em execução é justamente o caso que não funciona, e é
    # por isso que o publicador manda fechar o navegador.
    temporario = caminho + ".adv-br.tmp"
    with open(temporario, "w", encoding="utf-8") as f:
        f.write("\n\n".join(lista) + "\n\n")
    os.replace(temporario, caminho)


def main(argv):
    if len(argv) < 3:
        print(__doc__, file=sys.stderr)
        return 1

    acao, banco = argv[1], argv[2]
    arquivo = os.path.join(banco, "pkcs11.txt")

    if acao == "listar":
        for bloco in ler(arquivo):
            nome = campo(bloco, "name")
            if nome:
                print("%s\t%s" % (nome, campo(bloco, "library") or ""))
        return 0

    if acao == "remover":
        if len(argv) != 4:
            print("uso: nssdb.py remover <banco> <nome>", file=sys.stderr)
            return 1
        nome = argv[3]
        lista = ler(arquivo)
        restantes = [b for b in lista if campo(b, "name") != nome]
        if len(restantes) == len(lista):
            return 0
        escrever(arquivo, restantes)
        return 0

    if acao == "registrar":
        if len(argv) != 5:
            print("uso: nssdb.py registrar <banco> <nome> <biblioteca>",
                  file=sys.stderr)
            return 1
        nome, biblioteca = argv[3], argv[4]
        lista = ler(arquivo)
        if not lista:
            # Sem pkcs11.txt não há banco: criar um do zero significaria
            # escrever também o bloco do módulo interno do NSS, com parâmetros
            # que dependem do caminho. Um perfil que nunca foi aberto não tem
            # o que registrar.
            print("banco NSS sem pkcs11.txt: %s" % arquivo, file=sys.stderr)
            return 1

        novo = "library=%s\nname=%s\nNSS=trustOrder=100" % (biblioteca, nome)
        for i, bloco in enumerate(lista):
            if campo(bloco, "name") == nome:
                if bloco.strip() == novo:
                    return 0          # já está exatamente assim
                lista[i] = novo
                break
        else:
            lista.append(novo)
        escrever(arquivo, lista)
        return 0

    print("ação desconhecida: %s" % acao, file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
