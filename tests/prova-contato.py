"""O e-mail de contato do relato, contra os casos compartilhados com o serviço.

É o único campo do relato que vai sem sanitização. O que se confere: que o
aplicativo aceita e recusa o mesmo que servidor/contato.go, e que o contato
sai no pedido sem espaços nas pontas.
"""
import io
import json
import sys
import urllib.request

sys.path.insert(0, "ui")
import relator  # noqa: E402

CASOS = "tests/casos-contato.json"


def main():
    with open(CASOS, encoding="utf-8") as arquivo:
        casos = json.load(arquivo)

    falhas = 0
    if len(casos["validos"]) < 3 or len(casos["invalidos"]) < 5:
        print("  ERRO %s tem poucos casos" % CASOS)
        return 1
    for contato in casos["validos"] + [""]:
        if not relator.contato_valido(contato):
            print("  ERRO %r devia passar" % contato)
            falhas += 1
    for contato in casos["invalidos"]:
        if relator.contato_valido(contato):
            print("  ERRO %r devia ser recusado" % contato)
            falhas += 1

    enviado = {}

    class Resposta(io.BytesIO):
        def __enter__(self):
            return self

        def __exit__(self, *_):
            return False

    def falso_urlopen(pedido, timeout=None):
        enviado.update(json.loads(pedido.data.decode("utf-8")))
        return Resposta(b'{"situacao": "publicado", "url": "u"}')

    urllib.request.urlopen = falso_urlopen
    relator.enviar({}, "0", "t", "m", "d", "1.0", "  fulana@exemplo.com ")
    if enviado.get("contato") != "fulana@exemplo.com":
        print("  ERRO o pedido levou contato=%r" % enviado.get("contato"))
        falhas += 1
    relator.enviar({}, "0", "t", "m", "d", "1.0")
    if enviado.get("contato") != "":
        print("  ERRO sem contato, o pedido levou %r" % enviado.get("contato"))
        falhas += 1

    if not falhas:
        print("  ok  %d casos de contato" % (len(casos["validos"]) + len(casos["invalidos"])))
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
