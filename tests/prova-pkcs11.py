#!/usr/bin/env python3
"""Carrega um módulo PKCS#11 e lista os tokens que ele apresenta.

Existe porque contar módulos ou olhar se um socket existe não prova nada: o
Flatpak já põe um socket do p11-kit em todo sandbox, e um p11-kit-proxy sem
módulo nenhum falha igual a um driver quebrado. A prova é funcional --
C_Initialize, C_GetSlotList, C_GetTokenInfo -- e os rótulos que saem dizem de
onde o token veio.

Usa só ctypes porque o runtime do Flatpak não tem pkcs11-tool nem binutils.

Uso: prova-pkcs11.py <caminho-do-modulo.so>
"""
import ctypes
import sys

CKR_OK = 0
CKR_CRYPTOKI_ALREADY_INITIALIZED = 0x191


class CK_VERSION(ctypes.Structure):
    _fields_ = [("major", ctypes.c_ubyte), ("minor", ctypes.c_ubyte)]


# So os sete primeiros ponteiros da CK_FUNCTION_LIST interessam aqui, e a
# ordem deles e' fixada pelo padrao PKCS#11. O proxy do p11-kit exporta
# apenas C_GetFunctionList: os demais simbolos nao existem no .so e precisam
# vir desta tabela.
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
        ("C_GetSlotInfo", ctypes.CFUNCTYPE(
            ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p)),
        ("C_GetTokenInfo", ctypes.CFUNCTYPE(
            ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p)),
    ]


class CK_TOKEN_INFO(ctypes.Structure):
    _fields_ = [
        ("label", ctypes.c_ubyte * 32),
        ("manufacturerID", ctypes.c_ubyte * 32),
        ("model", ctypes.c_ubyte * 16),
        ("serialNumber", ctypes.c_ubyte * 16),
        ("flags", ctypes.c_ulong),
        ("ulMaxSessionCount", ctypes.c_ulong),
        ("ulSessionCount", ctypes.c_ulong),
        ("ulMaxRwSessionCount", ctypes.c_ulong),
        ("ulRwSessionCount", ctypes.c_ulong),
        ("ulMaxPinLen", ctypes.c_ulong),
        ("ulMinPinLen", ctypes.c_ulong),
        ("ulTotalPublicMemory", ctypes.c_ulong),
        ("ulFreePublicMemory", ctypes.c_ulong),
        ("ulTotalPrivateMemory", ctypes.c_ulong),
        ("ulFreePrivateMemory", ctypes.c_ulong),
        ("hardwareVersion", ctypes.c_ubyte * 2),
        ("firmwareVersion", ctypes.c_ubyte * 2),
        ("utcTime", ctypes.c_ubyte * 16),
    ]


def texto(campo):
    return bytes(campo).decode("utf-8", "replace").strip()


def main(caminho):
    lib = ctypes.CDLL(caminho)
    lib.C_GetFunctionList.argtypes = [ctypes.POINTER(ctypes.POINTER(CK_FUNCTION_LIST))]
    lib.C_GetFunctionList.restype = ctypes.c_ulong

    tabela = ctypes.POINTER(CK_FUNCTION_LIST)()
    rv = lib.C_GetFunctionList(ctypes.byref(tabela))
    if rv != CKR_OK:
        print(f"C_GetFunctionList falhou: 0x{rv:x}", file=sys.stderr)
        return 1
    fn = tabela.contents

    rv = fn.C_Initialize(None)
    if rv not in (CKR_OK, CKR_CRYPTOKI_ALREADY_INITIALIZED):
        print(f"C_Initialize falhou: 0x{rv:x}", file=sys.stderr)
        return 1

    contagem = ctypes.c_ulong(0)
    rv = fn.C_GetSlotList(1, None, ctypes.byref(contagem))
    if rv != CKR_OK:
        print(f"C_GetSlotList falhou: 0x{rv:x}", file=sys.stderr)
        return 1

    print(f"slots com token: {contagem.value}")
    if contagem.value == 0:
        return 1

    slots = (ctypes.c_ulong * contagem.value)()
    rv = fn.C_GetSlotList(1, slots, ctypes.byref(contagem))
    if rv != CKR_OK:
        print(f"C_GetSlotList (2) falhou: 0x{rv:x}", file=sys.stderr)
        return 1

    for slot in slots:
        info = CK_TOKEN_INFO()
        rv = fn.C_GetTokenInfo(slot, ctypes.byref(info))
        if rv != CKR_OK:
            print(f"  slot {slot}: C_GetTokenInfo 0x{rv:x}")
            continue
        print(f"  token: {texto(info.label)!r}  "
              f"fabricante: {texto(info.manufacturerID)!r}  "
              f"modelo: {texto(info.model)!r}  "
              f"serie: {texto(info.serialNumber)!r}")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    sys.exit(main(sys.argv[1]))
