#!/usr/bin/env python3
"""Abre um módulo PKCS#11 (o proxy do p11-kit, por padrão) e lista os tokens.

Uso: listar.py [modulo.so]
Imprime uma linha JSON com os tempos de C_Initialize e de C_GetSlotList, e os
rótulos dos tokens presentes. Não faz login: listar tokens não pede PIN.
"""
import ctypes as C
import json
import sys
import time

UL = C.c_ulong
P = C.c_void_p
NOMES = """C_Initialize C_Finalize C_GetInfo C_GetFunctionList C_GetSlotList
C_GetSlotInfo C_GetTokenInfo""".split()


class Fn(C.Structure):
    _fields_ = [("version", C.c_ubyte * 2)] + [(n, P) for n in NOMES]


def f(tab, nome, *tipos):
    return C.CFUNCTYPE(UL, *tipos)(getattr(tab, nome))


def main(modulo):
    saida = {}
    t0 = time.perf_counter()
    lib = C.CDLL(modulo)
    lib.C_GetFunctionList.argtypes = [C.POINTER(C.POINTER(Fn))]
    pt = C.POINTER(Fn)()
    rv = lib.C_GetFunctionList(C.byref(pt))
    t = pt.contents
    rv = f(t, "C_Initialize", P)(None)
    t1 = time.perf_counter()
    saida["init_ms"] = round((t1 - t0) * 1000, 1)
    saida["init_rv"] = hex(rv)
    if rv != 0:
        print(json.dumps(saida))
        return
    n = UL(0)
    lista = f(t, "C_GetSlotList", C.c_ubyte, C.POINTER(UL), C.POINTER(UL))
    rv = lista(1, None, C.byref(n))
    slots = (UL * n.value)()
    rv = lista(1, slots, C.byref(n))
    info = (C.c_ubyte * 256)()
    rotulos = []
    for s in slots[: n.value]:
        if f(t, "C_GetTokenInfo", UL, P)(s, info) == 0:
            rotulos.append(bytes(info[:32]).decode(errors="replace").strip())
    t2 = time.perf_counter()
    saida["slots_ms"] = round((t2 - t1) * 1000, 1)
    saida["slots_rv"] = hex(rv)
    saida["tokens"] = rotulos
    f(t, "C_Finalize", P)(None)
    saida["total_ms"] = round((time.perf_counter() - t0) * 1000, 1)
    print(json.dumps(saida, ensure_ascii=False))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "/usr/lib/p11-kit-proxy.so")
