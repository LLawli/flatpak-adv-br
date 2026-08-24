#!/usr/bin/env python3
"""Lista os tokens que o NSS enxerga a partir de um banco (cert9.db + pkcs11.txt).

É a camada que o Firefox, o Chrome e o Papers usam de verdade: eles não
carregam um módulo PKCS#11 por caminho, carregam o que está registrado no banco
NSS do perfil. Provar que o módulo carrega (tests/prova-pkcs11.py) não prova que
o aplicativo o enxerga; esta prova fecha essa lacuna.

Uso: prova-nss.py <diretório-do-banco>

Nenhum PIN é pedido em momento algum: listar tokens não exige login, e tentar
login em token de hardware gasta tentativa: num token com "final try", a
tentativa seguinte o bloqueia.
"""
import ctypes
import ctypes.util
import sys

SECSuccess = 0


class PK11SlotListElement(ctypes.Structure):
    pass


PK11SlotListElement._fields_ = [
    ("next", ctypes.POINTER(PK11SlotListElement)),
    ("prev", ctypes.POINTER(PK11SlotListElement)),
    ("slot", ctypes.c_void_p),
    ("refCount", ctypes.c_int),
]


class PK11SlotList(ctypes.Structure):
    _fields_ = [
        ("head", ctypes.POINTER(PK11SlotListElement)),
        ("tail", ctypes.POINTER(PK11SlotListElement)),
        ("lock", ctypes.c_void_p),
    ]


def carregar():
    for nome in ("libnss3.so", ctypes.util.find_library("nss3")):
        if not nome:
            continue
        try:
            return ctypes.CDLL(nome)
        except OSError:
            continue
    print("libnss3 não encontrada", file=sys.stderr)
    raise SystemExit(1)


def main(banco):
    nss = carregar()
    nss.NSS_Init.argtypes = [ctypes.c_char_p]
    nss.NSS_Init.restype = ctypes.c_int
    # CKM_INVALID_MECHANISM (0xffffffff) quer dizer "todos os tokens". Passar
    # 0 devolve só os dois slots internos do NSS, o que parece sucesso e não é.
    nss.PK11_GetAllTokens.argtypes = [ctypes.c_ulong, ctypes.c_int,
                                      ctypes.c_int, ctypes.c_void_p]
    nss.PK11_GetAllTokens.restype = ctypes.POINTER(PK11SlotList)
    nss.PK11_GetTokenName.argtypes = [ctypes.c_void_p]
    nss.PK11_GetTokenName.restype = ctypes.c_char_p
    nss.PK11_GetModuleID.argtypes = [ctypes.c_void_p]
    nss.PK11_IsHW.argtypes = [ctypes.c_void_p]

    if nss.NSS_Init(("sql:" + banco).encode()) != SECSuccess:
        print("NSS_Init falhou em %s" % banco, file=sys.stderr)
        return 1

    lista = nss.PK11_GetAllTokens(ctypes.c_ulong(0xFFFFFFFF), 0, 0, None)
    if not lista:
        print("PK11_GetAllTokens não devolveu nada", file=sys.stderr)
        return 1

    encontrados = []
    elemento = lista.contents.head
    while elemento:
        nome = nss.PK11_GetTokenName(elemento.contents.slot)
        if nome:
            encontrados.append(nome.decode("utf-8", "replace"))
        elemento = elemento.contents.next

    print("tokens visíveis pelo NSS: %d" % len(encontrados))
    for nome in encontrados:
        print("  %s" % nome)

    # Só o banco interno do NSS não prova nada: ele existe em todo perfil.
    externos = [n for n in encontrados
                if n not in ("NSS Certificate DB", "NSS Generic Crypto Services")]
    return 0 if externos else 1


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
