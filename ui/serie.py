"""A série do p11-kit de cada lado da ponte, e o que fazer quando divergem.

Este é o modo de falha mais caro do projeto, e o mais silencioso. A ponte
PKCS#11 é o remoting do p11-kit: o p11-kit do sistema executa um processo deste
aplicativo e conversa com ele por um pipe. O que trafega ali é a tabela de
funções PKCS#11 serializada, e as duas pontas precisam concordar sobre o
formato dela.

Quando não concordam, nada recusa a conexão. Os slots enumeram, o PIN é aceito,
a lista de certificados aparece inteira, e TODA assinatura falha com
CKR_DEVICE_ERROR. Como autenticar por certificado também exige assinar (no
CertificateVerify do handshake TLS), o login no Projudi e no eproc para de
funcionar com o certificado aparecendo normalmente na lista. Tudo parece certo
até o último passo.

O runtime traz a série 0.26. Debian trixie e Ubuntu 24.04 trazem a 0.25. Ver
docs/ARMADILHAS.md.
"""
import os
import subprocess

import catalogo
import instalador

# Quanto tempo esperar pelo adv-br-serie. Ele carrega um módulo PKCS#11 para
# perguntar a versão, o que envolve disco e não é instantâneo; e ele roda na
# abertura da janela, onde travar é pior que não saber.
ESPERA = 15


def _perguntar(qual):
    try:
        saida = subprocess.run(
            ["/app/bin/adv-br-serie", qual], capture_output=True, text=True,
            timeout=ESPERA, check=False)
    except (OSError, subprocess.SubprocessError):
        return ""
    return saida.stdout.strip()


def do_host():
    """A série do p11-kit do sistema, ou "" se não der para descobrir."""
    return _perguntar("host")


def do_pacote():
    """A série que a ponte deste aplicativo vai usar."""
    return _perguntar("pacote")


def componente_para(serie):
    """O componente de compatibilidade daquela série, se o catálogo tiver."""
    for componente in catalogo.CATALOGO:
        if componente.tipo == "compatibilidade" and componente.serie == serie:
            return componente
    return None


def pendencia():
    """O que falta para a ponte funcionar. None quando está tudo certo.

    Devolve (série do host, componente a instalar) quando as séries divergem e
    existe componente para resolver; (série do host, None) quando divergem e
    não existe, que é o caso em que só dá para avisar.

    Não perguntar é diferente de estar tudo bem: quando qualquer um dos dois
    lados não responde, esta função devolve None e o aplicativo segue como
    antes. Um aviso baseado em leitura falha seria pior que a ausência dele.
    """
    host = do_host()
    pacote = do_pacote()
    if not host or not pacote or host == pacote:
        return None
    componente = componente_para(host)
    if componente is not None and instalador.instalado(componente):
        # Instalado mas ainda divergente: a ponte usa o componente, então isto
        # significa que o componente é de outra série. Vale avisar.
        return (host, componente)
    return (host, componente)
