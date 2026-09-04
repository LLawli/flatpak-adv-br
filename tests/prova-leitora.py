#!/usr/bin/env python3
"""Diz se alguma leitora está travada a ponto de esconder TODOS os tokens.

A pergunta que todo programa faz é `C_GetSlotList(CKF_TOKEN_PRESENT)`: "quais
slots têm token?". Para respondê-la o p11-kit consulta cada slot, e um único
`CKR_DEVICE_ERROR` reprova a chamada INTEIRA. O efeito é desproporcional: uma
leitora que ninguém está usando apaga da lista todos os certificados, inclusive
os que não têm leitora nenhuma, como os em nuvem.

Engana porque a linha de comando desmente: `pkcs11-tool -L` pergunta SEM o
filtro e mostra tudo, então parece que está tudo bem enquanto o navegador, o
Papers e o PJeOffice não mostram nada.

Roda dentro do sandbox, sobre o p11-kit-proxy do runtime. Sai 0 quando está
tudo bem, 1 quando há leitora travada, 2 quando nem deu para perguntar.
"""
import ctypes
import glob
import os
import sys

CKR_OK = 0
CKR_CRYPTOKI_ALREADY_INITIALIZED = 0x191
CKF_TOKEN_PRESENT = 1


class CK_VERSION(ctypes.Structure):
    _fields_ = [("major", ctypes.c_ubyte), ("minor", ctypes.c_ubyte)]


class CK_FUNCTION_LIST(ctypes.Structure):
    _fields_ = [
        ("version", CK_VERSION),
        ("C_Initialize", ctypes.CFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)),
        ("C_Finalize", ctypes.CFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)),
        ("C_GetInfo", ctypes.CFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)),
        ("C_GetFunctionList", ctypes.CFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)),
        ("C_GetSlotList", ctypes.CFUNCTYPE(
            ctypes.c_ulong, ctypes.c_ubyte,
            ctypes.POINTER(ctypes.c_ulong), ctypes.POINTER(ctypes.c_ulong))),
    ]


def proxy():
    for candidato in (glob.glob("/usr/lib/*/p11-kit-proxy.so")
                      + ["/usr/lib/p11-kit-proxy.so"]):
        if os.path.exists(candidato):
            return candidato
    return None


def main():
    caminho = proxy()
    if not caminho:
        print("não achei o p11-kit-proxy do runtime", file=sys.stderr)
        return 2

    lib = ctypes.CDLL(caminho)
    lib.C_GetFunctionList.argtypes = [ctypes.POINTER(ctypes.POINTER(CK_FUNCTION_LIST))]
    tabela = ctypes.POINTER(CK_FUNCTION_LIST)()
    if lib.C_GetFunctionList(ctypes.byref(tabela)) != CKR_OK:
        print("C_GetFunctionList falhou", file=sys.stderr)
        return 2
    fn = tabela.contents
    if fn.C_Initialize(None) not in (CKR_OK, CKR_CRYPTOKI_ALREADY_INITIALIZED):
        print("C_Initialize falhou", file=sys.stderr)
        return 2

    quantos = ctypes.c_ulong(0)
    filtrada = fn.C_GetSlotList(CKF_TOKEN_PRESENT, None, ctypes.byref(quantos))
    if filtrada == CKR_OK:
        print("%d slot(s) com token" % quantos.value)
        return 0

    larga = fn.C_GetSlotList(0, None, ctypes.byref(quantos))
    if larga != CKR_OK:
        print("as duas perguntas falharam (0x%x e 0x%x)" % (filtrada, larga),
              file=sys.stderr)
        return 2

    print("a pergunta filtrada falhou com 0x%x, e a larga achou %d slot(s)"
          % (filtrada, quantos.value), file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
