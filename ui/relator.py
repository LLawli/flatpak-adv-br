"""Manda o relato ao serviço, com a prova de trabalho que ele exige.

A prova existe para o endereço de relato não virar um formulário aberto de
criar issues. Ela custa memória e alguns segundos, e nada disso é pedido à
pessoa: a janela começa a resolver quando o diálogo abre, e termina enquanto
ela escreve o que aconteceu.

O que se envia é exatamente o texto que a pessoa viu, e o serviço aplica as
mesmas regras de limpeza de novo, porque a versão do aplicativo que enviou não
é algo que ele controle.
"""
import hashlib
import json
import os
import urllib.error
import urllib.request

import registro

ORIGEM = "https://flatpak.lukakuuhaku.dev"
ESPERA = 30

# Teto de tentativas antes de desistir. Com a dificuldade padrão a média é de
# 128, e passar de alguns milhares significa que o servidor pediu algo que esta
# máquina não vai resolver em tempo aceitável: melhor dizer isso do que deixar
# a pessoa esperando para sempre.
TENTATIVAS_MAXIMAS = 20000


def _origem():
    return os.environ.get("ADV_BR_ORIGEM") or ORIGEM


def pedir_desafio():
    with urllib.request.urlopen(_origem() + "/api/desafio", timeout=ESPERA) as resposta:
        return json.loads(resposta.read().decode("utf-8"))


def _zeros_iniciais(dados):
    total = 0
    for byte in dados:
        if byte == 0:
            total += 8
            continue
        for deslocamento in range(7, -1, -1):
            if byte >> deslocamento & 1:
                return total
            total += 1
        return total
    return total


def resolver(desafio, parar=None):
    """Procura o nonce que satisfaz o desafio. Devolve None se pedirem parada.

    `parar` é chamado a cada tentativa: é como a janela cancela o trabalho
    quando a pessoa fecha o diálogo antes de enviar.
    """
    semente = desafio["semente"]
    dificuldade = int(desafio["dificuldade"])
    sal = semente.encode("utf-8")
    # maxmem precisa acompanhar os parâmetros que o servidor mandou, senão o
    # hashlib recusa com "memory limit exceeded" antes da primeira tentativa.
    memoria = int(desafio["n"]) * int(desafio["r"]) * 256

    for tentativa in range(TENTATIVAS_MAXIMAS):
        if parar is not None and parar():
            return None
        saida = hashlib.scrypt(
            ("%s:%d" % (semente, tentativa)).encode("utf-8"), salt=sal,
            n=int(desafio["n"]), r=int(desafio["r"]), p=int(desafio["p"]),
            dklen=32, maxmem=memoria)
        if _zeros_iniciais(saida) >= dificuldade:
            return str(tentativa)

    registro.registrar("desisti da prova de trabalho após %d tentativas",
                       TENTATIVAS_MAXIMAS)
    return None


def enviar(desafio, nonce, titulo, mensagem, diagnostico, versao=""):
    """Envia o relato. Devolve (situação, detalhe).

    Situações: "publicado" com a URL, "guardado" quando o serviço aceitou mas o
    GitHub não estava disponível, e "erro" com o motivo.
    """
    corpo = json.dumps({
        "desafio": desafio, "nonce": nonce, "titulo": titulo,
        "mensagem": mensagem, "diagnostico": diagnostico, "versao": versao,
    }).encode("utf-8")

    pedido = urllib.request.Request(
        _origem() + "/api/relato", data=corpo,
        headers={"Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(pedido, timeout=ESPERA) as resposta:
            devolvido = json.loads(resposta.read().decode("utf-8"))
    except urllib.error.HTTPError as erro:
        detalhe = erro.read().decode("utf-8", "replace")[:300]
        registro.registrar("relato recusado (%s): %s", erro.code, detalhe)
        try:
            detalhe = json.loads(detalhe).get("erro", detalhe)
        except ValueError:
            pass
        return "erro", detalhe
    except OSError as erro:
        # Sem rede é o caso mais comum, e a mensagem do sistema não ajuda
        # ninguém: "Network is unreachable" não diz o que fazer.
        registro.falha("relato não chegou ao servidor", erro)
        return "erro", ("não consegui falar com o servidor. Confira a sua "
                        "conexão e tente de novo.")

    situacao = devolvido.get("situacao", "?")
    if situacao == "publicado":
        return "publicado", devolvido.get("url", "")
    if situacao == "repetido":
        return "publicado", ""
    return situacao, ""
