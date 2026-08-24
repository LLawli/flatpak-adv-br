#!/usr/bin/env python3
"""Tira de um override de Flatpak apenas o que este projeto pôs.

Por que não `flatpak override --nofilesystem=...`: esse comando não remove a
permissão, ele grava uma NEGAÇÃO explícita (`filesystems=!xdg-run/...`), que é
um estado diferente de "nunca foi concedida" e fica no arquivo para sempre. E
`--reset` vai longe demais: zera também o que outro programa concedeu, como os
caminhos que o KeePassXC pede ao navegador.

Este script edita o arquivo de override do usuário removendo uma chave de cada
vez, e apaga o arquivo se ele ficar sem nada.

Uso:
    overrides.py remover <app-id> filesystem <valor>
    overrides.py remover <app-id> talk-name <nome>
"""
import configparser
import os
import sys


def caminho_do_override(app):
    base = os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share")
    return os.path.join(base, "flatpak", "overrides", app)


def carregar(arquivo):
    # O formato é INI, mas as chaves têm maiúsculas que importam e valores
    # repetidos separados por ";" — o configparser serve desde que não
    # normalize os nomes.
    parser = configparser.ConfigParser()
    parser.optionxform = str
    parser.read(arquivo, encoding="utf-8")
    return parser


def salvar(arquivo, parser):
    # Uma seção vazia deixa lixo no arquivo; um arquivo vazio deixa lixo no
    # diretório de overrides.
    for secao in list(parser.sections()):
        if not parser.items(secao):
            parser.remove_section(secao)
    if not parser.sections():
        os.remove(arquivo)
        return "arquivo removido"
    with open(arquivo, "w", encoding="utf-8") as f:
        parser.write(f, space_around_delimiters=False)
    return "chave removida"


def remover(app, tipo, valor):
    arquivo = caminho_do_override(app)
    if not os.path.exists(arquivo):
        return "sem override"

    parser = carregar(arquivo)

    if tipo == "filesystem":
        if not parser.has_option("Context", "filesystems"):
            return "nada a remover"
        atuais = [v for v in parser.get("Context", "filesystems").split(";") if v]
        # A concessão pode estar como valor puro ou negada com "!".
        restantes = [v for v in atuais if v.lstrip("!") != valor]
        if len(restantes) == len(atuais):
            return "nada a remover"
        if restantes:
            parser.set("Context", "filesystems", ";".join(restantes) + ";")
        else:
            parser.remove_option("Context", "filesystems")
    elif tipo == "talk-name":
        secao = "Session Bus Policy"
        if not parser.has_option(secao, valor):
            return "nada a remover"
        parser.remove_option(secao, valor)
    else:
        raise SystemExit("tipo desconhecido: %s" % tipo)

    return salvar(arquivo, parser)


if __name__ == "__main__":
    if len(sys.argv) != 5 or sys.argv[1] != "remover":
        print(__doc__, file=sys.stderr)
        raise SystemExit(2)
    print(remover(sys.argv[2], sys.argv[3], sys.argv[4]))
