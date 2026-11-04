"""Lista os tokens que o NSS de um consumidor enxerga, com o módulo de cada um.

Roda dentro do ambiente do consumidor (o sandbox de um aplicativo Flatpak, ou
o host), lida pela entrada padrão: o Âncora a embute e a passa assim, sem
depender de arquivo nenhum do lado de lá.

Uso: python3 - <libnss3> <banco> <raizes> [servidor]

raizes: "auto" abre o banco como o Firefox, deixando o NSS carregar sozinho o
módulo de raízes; "chromium" abre como o Chromium e o poppler, que carregam o
libnssckbi.so eles mesmos, como "Root Certs".
servidor: aponta o cliente do p11-kit para outro socket (só em testes).

Imprime uma linha JSON: [[módulo, token], ...]. Abre o banco só para leitura
e não pede PIN.
"""
import ctypes
import json
import os
import sys

NSS_INIT_READONLY = 0x1
NSS_INIT_NOROOTINIT = 0x10


class Elemento(ctypes.Structure):
    pass


Elemento._fields_ = [
    ("next", ctypes.POINTER(Elemento)),
    ("prev", ctypes.POINTER(Elemento)),
    ("slot", ctypes.c_void_p),
    ("refCount", ctypes.c_int),
]


class Lista(ctypes.Structure):
    _fields_ = [("head", ctypes.POINTER(Elemento)), ("tail", ctypes.POINTER(Elemento)),
                ("lock", ctypes.c_void_p)]


class Modulo(ctypes.Structure):
    # O começo de SECMODModule (secmodt.h).
    _fields_ = [("arena", ctypes.c_void_p), ("internal", ctypes.c_int),
                ("loaded", ctypes.c_int), ("isFIPS", ctypes.c_int),
                ("dllName", ctypes.c_char_p), ("commonName", ctypes.c_char_p)]


def texto(b):
    return (b or b"").decode("utf-8", "replace")


def main(libnss3, banco, raizes, servidor=None):
    if servidor:
        os.environ["P11_KIT_SERVER_ADDRESS"] = "unix:path=" + servidor
    nss = ctypes.CDLL(libnss3)
    nss.NSS_Initialize.argtypes = [ctypes.c_char_p] * 4 + [ctypes.c_uint32]
    nss.SECMOD_LoadUserModule.argtypes = [ctypes.c_char_p, ctypes.c_void_p, ctypes.c_int]
    nss.SECMOD_LoadUserModule.restype = ctypes.c_void_p
    nss.PK11_GetAllTokens.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    nss.PK11_GetAllTokens.restype = ctypes.POINTER(Lista)
    nss.PK11_GetTokenName.argtypes = [ctypes.c_void_p]
    nss.PK11_GetTokenName.restype = ctypes.c_char_p
    nss.PK11_GetModule.argtypes = [ctypes.c_void_p]
    nss.PK11_GetModule.restype = ctypes.POINTER(Modulo)

    flags = NSS_INIT_READONLY
    if raizes == "chromium":
        flags |= NSS_INIT_NOROOTINIT
    if nss.NSS_Initialize(("sql:" + banco).encode(), b"", b"", b"secmod.db", flags) != 0:
        print(json.dumps({"erro": "NSS_Initialize falhou em " + banco}))
        return 1
    if raizes == "chromium":
        nss.SECMOD_LoadUserModule(b'library="libnssckbi.so" name="Root Certs"', None, 0)

    # CKM_INVALID_MECHANISM: todos os tokens. Com 0 vêm só os internos do NSS.
    lista = nss.PK11_GetAllTokens(0xFFFFFFFF, 0, 0, None)
    saida = []
    e = lista.contents.head if lista else None
    while e:
        slot = e.contents.slot
        saida.append([texto(nss.PK11_GetModule(slot).contents.commonName),
                      texto(nss.PK11_GetTokenName(slot))])
        e = e.contents.next
    print(json.dumps(saida, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
