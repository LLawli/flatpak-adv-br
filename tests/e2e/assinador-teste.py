#!/usr/bin/env python3
"""Um assinador de teste que fala native messaging como os de verdade.

Lê uma mensagem (4 bytes de tamanho, little-endian, e o JSON), carrega pelo
nome a biblioteca PKCS#11 que a extensão do Lacuna e do Softplan pedem
(libaetpkss.so.3), lista os tokens e responde com eles e a HOME em que rodou.
"""
import ctypes as C
import json
import os
import struct
import sys

UL = C.c_ulong
P = C.c_void_p
NOMES = """C_Initialize C_Finalize C_GetInfo C_GetFunctionList C_GetSlotList
C_GetSlotInfo C_GetTokenInfo""".split()


class Fn(C.Structure):
    _fields_ = [("version", C.c_ubyte * 2)] + [(n, P) for n in NOMES]


def f(tab, nome, *tipos):
    return C.CFUNCTYPE(UL, *tipos)(getattr(tab, nome))


def tokens():
    lib = C.CDLL("libaetpkss.so.3")
    pt = C.POINTER(Fn)()
    lib.C_GetFunctionList(C.byref(pt))
    t = pt.contents
    f(t, "C_Initialize", P)(None)
    n = UL(0)
    lista = f(t, "C_GetSlotList", C.c_ubyte, C.POINTER(UL), C.POINTER(UL))
    lista(1, None, C.byref(n))
    slots = (UL * n.value)()
    lista(1, slots, C.byref(n))
    info = (C.c_ubyte * 256)()
    saida = []
    for s in slots[: n.value]:
        if f(t, "C_GetTokenInfo", UL, P)(s, info) == 0:
            rotulo = bytes(info[:32]).decode("utf-8", "replace").strip()
            if rotulo:
                saida.append(rotulo)
    return saida


def main():
    tamanho = struct.unpack("<I", sys.stdin.buffer.read(4))[0]
    pedido = json.loads(sys.stdin.buffer.read(tamanho))
    try:
        resposta = {"pedido": pedido, "tokens": tokens(), "home": os.environ.get("HOME")}
    except OSError as e:
        resposta = {"erro": str(e)}
    corpo = json.dumps(resposta).encode()
    sys.stdout.buffer.write(struct.pack("<I", len(corpo)) + corpo)
    sys.stdout.buffer.flush()


if __name__ == "__main__":
    main()
