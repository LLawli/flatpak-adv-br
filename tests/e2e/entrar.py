#!/usr/bin/env python3
"""Faz login (caminho de autenticação protegido, sem PIN) no token ESPERA pelo
proxy do p11-kit, e diz quanto levou e o que voltou. É o "aplicativo
assinando" do teste da supervisão.
"""
import ctypes as C
import json
import time

UL = C.c_ulong
P = C.c_void_p
NOMES = """C_Initialize C_Finalize C_GetInfo C_GetFunctionList C_GetSlotList
C_GetSlotInfo C_GetTokenInfo C_GetMechanismList C_GetMechanismInfo C_InitToken
C_InitPIN C_SetPIN C_OpenSession C_CloseSession C_CloseAllSessions
C_GetSessionInfo C_GetOperationState C_SetOperationState C_Login""".split()


class Fn(C.Structure):
    _fields_ = [("version", C.c_ubyte * 2)] + [(n, P) for n in NOMES]


def f(tab, nome, *tipos):
    return C.CFUNCTYPE(UL, *tipos)(getattr(tab, nome))


lib = C.CDLL("/usr/lib/p11-kit-proxy.so")
pt = C.POINTER(Fn)()
lib.C_GetFunctionList(C.byref(pt))
t = pt.contents
f(t, "C_Initialize", P)(None)
n = UL(0)
f(t, "C_GetSlotList", C.c_ubyte, C.POINTER(UL), C.POINTER(UL))(1, None, C.byref(n))
slots = (UL * n.value)()
f(t, "C_GetSlotList", C.c_ubyte, C.POINTER(UL), C.POINTER(UL))(1, slots, C.byref(n))
info = (C.c_ubyte * 256)()
alvo = None
for s in slots[: n.value]:
    f(t, "C_GetTokenInfo", UL, P)(s, info)
    if bytes(info[:32]).decode().strip() == "ESPERA":
        alvo = s
if alvo is None:
    print(json.dumps({"erro": "token ESPERA ausente"}))
    raise SystemExit(1)
h = UL(0)
f(t, "C_OpenSession", UL, UL, P, P, C.POINTER(UL))(alvo, 4, None, None, C.byref(h))
t0 = time.monotonic()
rv = f(t, "C_Login", UL, UL, C.c_char_p, UL)(h, 1, None, 0)
print(json.dumps({"login_rv": hex(rv), "segundos": round(time.monotonic() - t0, 1)}))
