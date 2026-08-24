"""Registra os componentes instalados no p11-kit do sandbox e lê os tokens.

O aplicativo precisa responder uma pergunta só, e ela é a que o usuário faz:
"o meu certificado apareceu?". Para isso, cada driver instalado vira um
`.module` em /etc/pkcs11/modules, que é um tmpfs recriado a cada execução, e
depois se pergunta ao p11-kit quais tokens existem.

Perguntar é chamar: contar módulos ou olhar se um arquivo existe não diz nada.
Aqui se chama C_GetSlotList e se lê o rótulo de cada token, que é justamente o
nome que a pessoa reconhece.
"""
import ctypes
import glob
import os

import instalador

MODULOS = "/etc/pkcs11/modules"
CKR_OK = 0
CKR_CRYPTOKI_ALREADY_INITIALIZED = 0x191
CKF_TOKEN_PRESENT = 1


class CK_VERSION(ctypes.Structure):
    _fields_ = [("major", ctypes.c_ubyte), ("minor", ctypes.c_ubyte)]


class CK_FUNCTION_LIST(ctypes.Structure):
    """Só os sete primeiros ponteiros interessam, e a ordem é a do padrão.

    O p11-kit-proxy exporta apenas C_GetFunctionList: os demais símbolos não
    existem no .so e têm de sair daqui.
    """
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
        ("resto", ctypes.c_ubyte * 200),
    ]


def _texto(campo):
    return bytes(campo).decode("utf-8", "replace").strip().strip("\x00").strip()


# Módulos que vêm no próprio aplicativo. O OpenSC está aqui e não entre os
# componentes porque ele é livre e pode ser embutido: é ele que faz o
# aplicativo já servir para alguma coisa assim que instala, antes de a pessoa
# baixar driver nenhum.
#
# O p11-kit do sandbox lê /etc/pkcs11/modules e /usr/share/p11-kit/modules, e
# nenhum dos dois é /app: sem registrar, o OpenSC do pacote simplesmente não
# existe para quem pergunta.
MODULOS_DO_APP = ["/app/lib/opensc-pkcs11.so"]


def modulos_instalados():
    """Caminhos dos módulos PKCS#11 que o aplicativo enxerga.

    Os que vêm nele e os dos componentes que a pessoa instalou.
    """
    encontrados = [caminho for caminho in MODULOS_DO_APP if os.path.exists(caminho)]
    encontrados += sorted(
        glob.glob(os.path.join(instalador.raiz(), "*", "pkcs11", "*.so")))
    return encontrados


def registrar():
    """Escreve um .module por driver instalado. Devolve quantos foram."""
    if not os.access(MODULOS, os.W_OK):
        return 0
    quantos = 0
    for caminho in modulos_instalados():
        # O nome do .module sai do componente, quando o módulo vem de um; do
        # arquivo, quando vem do próprio aplicativo.
        if caminho.startswith(instalador.raiz()):
            nome = os.path.basename(os.path.dirname(os.path.dirname(caminho)))
        else:
            nome = os.path.splitext(os.path.basename(caminho))[0]

        # critical: no — com mais de um driver, um deles recusar o cartão
        # presente é o caso comum, não a exceção, e não pode derrubar os outros.
        with open(os.path.join(MODULOS, "adv-br-%s.module" % nome), "w",
                  encoding="utf-8") as arquivo:
            arquivo.write("module: %s\ncritical: no\n" % caminho)
        quantos += 1
    return quantos


def _proxy():
    """O p11-kit-proxy do runtime, para uso dentro do sandbox."""
    for candidato in glob.glob("/usr/lib/*/p11-kit-proxy.so") + ["/usr/lib/p11-kit-proxy.so"]:
        if os.path.exists(candidato):
            return candidato
    return None


# Onde procurar o p11-kit-proxy do host, em /run/host/usr, que é onde o
# --filesystem=host-os monta o /usr de lá.
#
# O que se grava no banco NSS é o caminho como o host o vê, sem o /run/host: é
# o programa de lá que vai abrir o arquivo.
PREFIXO_HOST = "/run/host"
CANDIDATOS_PROXY = [
    "/usr/lib64/p11-kit-proxy.so",                    # Fedora, openSUSE
    "/usr/lib/x86_64-linux-gnu/p11-kit-proxy.so",     # Debian, Ubuntu
    "/usr/lib/p11-kit-proxy.so",                      # Arch
]


def proxy_do_host():
    """O p11-kit-proxy do host, ou None se não der para encontrar.

    Um só, e não todos os que existirem: em distribuição onde /usr/lib64 é link
    para /usr/lib, dois candidatos apontam para o mesmo arquivo, e registrar os
    dois faz cada token aparecer duas vezes na lista do navegador.
    """
    for candidato in CANDIDATOS_PROXY:
        if os.path.exists(PREFIXO_HOST + candidato):
            return candidato
    return None


def tokens():
    """Rótulos dos tokens presentes, pelo proxy do p11-kit.

    Nenhum login é feito: listar não exige PIN, e cada tentativa errada gasta
    uma das poucas que um token de hardware tem.
    """
    caminho = _proxy()
    if not caminho:
        return []

    lib = ctypes.CDLL(caminho)
    lib.C_GetFunctionList.argtypes = [ctypes.POINTER(ctypes.POINTER(CK_FUNCTION_LIST))]
    lib.C_GetFunctionList.restype = ctypes.c_ulong

    tabela = ctypes.POINTER(CK_FUNCTION_LIST)()
    if lib.C_GetFunctionList(ctypes.byref(tabela)) != CKR_OK:
        return []
    fn = tabela.contents

    if fn.C_Initialize(None) not in (CKR_OK, CKR_CRYPTOKI_ALREADY_INITIALIZED):
        return []

    contagem = ctypes.c_ulong(0)
    if fn.C_GetSlotList(CKF_TOKEN_PRESENT, None, ctypes.byref(contagem)) != CKR_OK:
        return []
    if contagem.value == 0:
        return []

    slots = (ctypes.c_ulong * contagem.value)()
    if fn.C_GetSlotList(CKF_TOKEN_PRESENT, slots, ctypes.byref(contagem)) != CKR_OK:
        return []

    encontrados = []
    for slot in slots:
        info = CK_TOKEN_INFO()
        if fn.C_GetTokenInfo(slot, ctypes.byref(info)) != CKR_OK:
            continue
        rotulo = _texto(info.label)
        # O módulo de confiança do Flatpak aparece aqui como se fosse token, e
        # não é: são as âncoras de CA do sistema.
        if rotulo in ("System Trust", "Default Trust"):
            continue
        encontrados.append({
            "rotulo": rotulo,
            "fabricante": _texto(info.manufacturerID),
            "modelo": _texto(info.model),
        })
    return encontrados
