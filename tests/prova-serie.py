"""O catálogo de compatibilidade do p11-kit, conferido contra os artefatos.

Cada componente de compatibilidade promete uma série. Se a promessa não bater
com o arquivo publicado, o efeito é o pior possível: a pessoa instala o ajuste,
a janela diz que está tudo certo, e nenhuma assinatura funciona.

Aqui se confere o que dá para conferir sem rede: que cada série do
packaging/p11kit-series.txt tem um componente, que cada componente tem sha256,
e que o arquivo em dist/, quando existir, tem o sha256 que o catálogo declara.
A prova de que o conteúdo reporta a série certa é feita com o pacote instalado:

    ADV_BR_TRUST_HOST=<trust do artefato> adv-br-serie host
"""
import hashlib
import os
import sys

sys.path.insert(0, "ui")
import catalogo  # noqa: E402
import serie as modulo_serie  # noqa: E402

TABELA = "packaging/p11kit-series.txt"


def series_conhecidas():
    series = []
    with open(TABELA, encoding="utf-8") as arquivo:
        for linha in arquivo:
            linha = linha.strip()
            if not linha or linha.startswith("#"):
                continue
            series.append(linha.split()[0])
    return series


def main():
    falhas = 0
    esperadas = series_conhecidas()
    if len(esperadas) < 3:
        # Piso absoluto: um arquivo ilegível faria este teste passar sem
        # conferir série nenhuma.
        print("  ERRO %s tem só %d séries" % (TABELA, len(esperadas)))
        return 1

    for numero in esperadas:
        componente = modulo_serie.componente_para(numero)
        if componente is None:
            print("  ERRO a série %s está em %s e não tem componente no catálogo"
                  % (numero, TABELA))
            falhas += 1
            continue

        fonte = componente.fontes[0] if componente.fontes else None
        if fonte is None or len(fonte.sha256) != 64:
            print("  ERRO %s: sha256 ausente ou inválido" % componente.chave)
            falhas += 1
            continue

        arquivo = os.path.join("dist", os.path.basename(fonte.url))
        if not os.path.exists(arquivo):
            print("  ok  %-12s série %-5s (dist/ ainda não tem o arquivo)"
                  % (componente.chave, numero))
            continue

        with open(arquivo, "rb") as f:
            digest = hashlib.sha256(f.read()).hexdigest()
        if digest != fonte.sha256:
            print("  ERRO %s: o arquivo em dist/ não confere com o catálogo\n"
                  "       catálogo: %s\n       arquivo : %s"
                  % (componente.chave, fonte.sha256, digest))
            falhas += 1
        else:
            print("  ok  %-12s série %-5s sha256 confere com dist/"
                  % (componente.chave, numero))

    # A série do runtime não deve ter componente: instalá-la seria trocar a
    # biblioteca por uma igual, e a janela nunca a ofereceria.
    if modulo_serie.componente_para("0.26") is not None:
        print("  ERRO existe componente para a série do próprio runtime")
        falhas += 1
    else:
        print("  ok  nenhum componente para a série do runtime")

    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
