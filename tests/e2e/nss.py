#!/usr/bin/env python3
"""Lista, pelo NSS, os tokens que um banco (perfil) enxerga, com o módulo de
cada um. É a camada que o Firefox usa de verdade: ele carrega o que está
registrado no pkcs11.txt do perfil.

Uso: nss.py <banco> [socket-do-servidor]

Com o socket, aponta o cliente do p11-kit para ele antes de abrir o NSS (o
Flatpak apaga P11_KIT_SERVER_ADDRESS ao abrir o sandbox, mas não depois).
Imprime uma linha JSON: [[módulo, token], ...]. Não pede PIN.
"""
import ctypes
import json
import os
import sys


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
    # O começo de SECMODModule (secmodt.h): o que vem depois não interessa.
    _fields_ = [("arena", ctypes.c_void_p), ("internal", ctypes.c_int),
                ("loaded", ctypes.c_int), ("isFIPS", ctypes.c_int),
                ("dllName", ctypes.c_char_p), ("commonName", ctypes.c_char_p)]


def main(banco, socket=None):
    if socket:
        os.environ["P11_KIT_SERVER_ADDRESS"] = "unix:path=" + socket
    nss = ctypes.CDLL("libnss3.so")
    nss.NSS_Init.argtypes = [ctypes.c_char_p]
    nss.PK11_GetAllTokens.argtypes = [ctypes.c_ulong, ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    nss.PK11_GetAllTokens.restype = ctypes.POINTER(Lista)
    nss.PK11_GetTokenName.argtypes = [ctypes.c_void_p]
    nss.PK11_GetTokenName.restype = ctypes.c_char_p
    nss.PK11_GetModule.argtypes = [ctypes.c_void_p]
    nss.PK11_GetModule.restype = ctypes.POINTER(Modulo)
    if nss.NSS_Init(("sql:" + banco).encode()) != 0:
        print(json.dumps({"erro": "NSS_Init"}))
        return 1
    # CKM_INVALID_MECHANISM: todos os tokens. Com 0 vêm só os internos do NSS.
    lista = nss.PK11_GetAllTokens(0xFFFFFFFF, 0, 0, None)
    saida = []
    e = lista.contents.head if lista else None
    while e:
        slot = e.contents.slot
        modulo = nss.PK11_GetModule(slot).contents.commonName or b""
        saida.append([modulo.decode("utf-8", "replace"),
                      (nss.PK11_GetTokenName(slot) or b"").decode("utf-8", "replace")])
        e = e.contents.next
    print(json.dumps(saida, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:]))
