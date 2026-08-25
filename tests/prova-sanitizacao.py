"""A sanitização do aplicativo, contra os casos compartilhados com o serviço."""
import json
import sys

sys.path.insert(0, "ui")
import sanitizar  # noqa: E402

CASOS = "tests/casos-sanitizacao.json"


def main():
    with open(CASOS, encoding="utf-8") as arquivo:
        casos = json.load(arquivo)["casos"]

    if len(casos) < 5:
        print("  ERRO %s tem só %d casos" % (CASOS, len(casos)))
        return 1

    falhas = 0
    for caso in casos:
        saida = sanitizar.sanitizar(caso["entrada"])
        for proibido in caso["some"]:
            if proibido in saida:
                print("  ERRO %s: %r sobreviveu em %r" % (caso["nome"], proibido, saida))
                falhas += 1
        for necessario in caso["fica"]:
            if necessario not in saida:
                print("  ERRO %s: a limpeza comeu %r de %r"
                      % (caso["nome"], necessario, saida))
                falhas += 1
    if not falhas:
        print("  ok  %d casos de sanitização" % len(casos))
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
