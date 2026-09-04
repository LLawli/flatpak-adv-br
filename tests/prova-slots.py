"""Um slot com defeito não pode esconder os que funcionam.

A janela responde uma pergunta só, e é a que a pessoa faz: "o meu certificado
apareceu?". Quem responde é o proxy do p11-kit, e ele tem um jeito de errar que
não parece erro: basta UM slot recusar para `C_GetSlotList(CKF_TOKEN_PRESENT)`
reprovar a chamada inteira, e a lista volta VAZIA — sem nenhum token, nem os que
estão perfeitamente vivos.

Aconteceu de verdade: uma YubiKey em modo OTP+FIDO+CCID com o scdaemon do gnupg
segurando a interface faz o OpenSC responder CKR_DEVICE_ERROR naquele slot. O
certificado em nuvem, que não tem leitora nenhuma e não podia se importar menos,
sumia da janela junto.

Aqui não há proxy, nem token, nem leitora: há uma tabela de funções de mentira
que responde o que se mandar responder. É o suficiente, porque o que se confere
é a DECISÃO.
"""
import ctypes
import os
import sys

sys.path.insert(0, os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "ui"))

import pkcs11  # noqa: E402

CKR_OK = 0
CKR_DEVICE_ERROR = 0x30


class TabelaDeMentira:
    """Responde C_GetSlotList como um proxy responderia, e anota o que ouviu.

    `com_token` é o que ele devolve à pergunta filtrada: uma lista de slots, ou
    um código de erro. `todos` é o que devolve à pergunta larga.
    """

    def __init__(self, com_token, todos):
        self.com_token = com_token
        self.todos = todos
        self.perguntas = []

    def C_GetSlotList(self, bandeira, saida, contagem):  # noqa: N802
        resposta = self.com_token if bandeira else self.todos
        if saida is None:
            self.perguntas.append(bandeira)
        if isinstance(resposta, int):
            return resposta
        if saida is None:
            contagem._obj.value = len(resposta)
            return CKR_OK
        for i, slot in enumerate(resposta):
            saida[i] = slot
        contagem._obj.value = len(resposta)
        return CKR_OK


def conferir():
    problemas = []

    # 1. O caso bom: a pergunta filtrada responde, e ninguém pergunta de novo.
    tabela = TabelaDeMentira(com_token=[7, 9], todos=[7, 9, 11])
    if pkcs11.slots_tolerantes(tabela) != [7, 9]:
        problemas.append("com a pergunta filtrada funcionando, deveria bastar ela")
    if len(tabela.perguntas) != 1:
        problemas.append("perguntou %d vezes quando uma bastava" % len(tabela.perguntas))

    # 2. O caso que motivou tudo: a filtrada quebra por causa de um slot, e a
    #    larga tem de ser tentada.
    tabela = TabelaDeMentira(com_token=CKR_DEVICE_ERROR, todos=[17, 18])
    if pkcs11.slots_tolerantes(tabela) != [17, 18]:
        problemas.append(
            "com a pergunta filtrada falhando, os slots vivos deveriam vir pela larga")

    # 3. Nenhum token espetado não é erro, e não pode virar uma segunda pergunta.
    tabela = TabelaDeMentira(com_token=[], todos=[3])
    if pkcs11.slots_tolerantes(tabela) != []:
        problemas.append("sem token espetado, a resposta é lista vazia")
    if len(tabela.perguntas) != 1:
        problemas.append("lista vazia não é falha; não devia perguntar de novo")

    # 4. E o achado precisa SAIR daqui. Contornar em silêncio é meia
    #    correção: quem faz a pergunta normal — o navegador, o Papers, o
    #    PJeOffice — continua cego, e a pessoa relata "o certificado não
    #    aparece" enquanto este aplicativo mostra a lista completa.
    tabela = TabelaDeMentira(com_token=CKR_DEVICE_ERROR, todos=[17, 18])
    pkcs11.slots_tolerantes(tabela)
    if not pkcs11.ULTIMO_AVISO:
        problemas.append("contornou a leitora travada sem registrar o achado")
    elif "scdaemon" not in pkcs11.ULTIMO_AVISO:
        problemas.append("o aviso não diz o que fazer a respeito")

    #    E, no caso bom, não pode sobrar aviso de uma chamada anterior.
    tabela = TabelaDeMentira(com_token=[7], todos=[7])
    pkcs11.slots_tolerantes(tabela)
    if pkcs11.ULTIMO_AVISO:
        problemas.append("avisou sobre leitora travada quando não havia nenhuma")

    # 5. E quando as duas quebram, quem chama precisa distinguir de "não há
    #    token": None, e não lista vazia.
    tabela = TabelaDeMentira(com_token=CKR_DEVICE_ERROR, todos=CKR_DEVICE_ERROR)
    if pkcs11.slots_tolerantes(tabela) is not None:
        problemas.append("com as duas perguntas falhando, a resposta é None")

    return problemas


def main():
    problemas = conferir()
    for problema in problemas:
        print("  " + problema, file=sys.stderr)
    if not problemas:
        print("  ok  um slot com defeito não esconde os que funcionam")
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
