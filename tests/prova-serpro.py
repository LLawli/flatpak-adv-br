"""A lista de drivers do Assinador Serpro, contra uma casa de mentira.

O arquivo é de quem instalou o Assinador Serpro e pode ter linha posta à mão. O
que estes casos guardam é que só a nossa chave seja tocada, que publicar duas
vezes não duplique nada, e que a detecção não invente o programa onde ele não
está: escrever a lista sem ele instalado é o mesmo erro que criar o diretório
de um navegador que a máquina não tem.
"""
import os
import shutil
import sys
import tempfile

sys.path.insert(0, "ui")
import publicador  # noqa: E402

DELES = "outro-driver=/opt/outro/libqualquer.so"
PROXY = "/usr/lib64/p11-kit-proxy.so"


def linhas():
    with open(publicador.LISTA_SERPRO, encoding="utf-8") as arquivo:
        return arquivo.read().splitlines()


def main():
    casa = tempfile.mkdtemp(prefix="prova-serpro.")
    # O módulo resolve o caminho na importação, como o aplicativo faz: trocar a
    # variável é o que põe a casa de mentira no lugar da de verdade.
    publicador.LISTA_SERPRO = os.path.join(casa, ".signer", "drivers.properties")
    falhas = 0
    try:
        def conferir(condicao, descricao):
            nonlocal falhas
            if condicao:
                print("  ok  %s" % descricao)
            else:
                print("  ERRO %s" % descricao)
                falhas += 1

        conferir(not publicador.assinador_serpro_presente(),
                 "sem o diretório de configuração, o Serpro não está aqui")
        conferir(not publicador.remover_da_lista_serpro(),
                 "remover não inventa o que não há")

        os.makedirs(os.path.dirname(publicador.LISTA_SERPRO))
        conferir(publicador.assinador_serpro_presente(),
                 "o diretório de configuração basta para reconhecê-lo")

        with open(publicador.LISTA_SERPRO, "w", encoding="utf-8") as arquivo:
            arquivo.write(DELES + "\n")

        conferir(publicador.escrever_na_lista_serpro(PROXY),
                 "a primeira escrita diz que mudou algo")
        conferir("%s=%s" % (publicador.CHAVE_SERPRO, PROXY) in linhas()
                 and DELES in linhas(),
                 "escreve a nossa linha e deixa a de outro driver onde estava")

        conferir(not publicador.escrever_na_lista_serpro(PROXY),
                 "escrever de novo diz que não havia o que fazer")
        conferir(len([l for l in linhas()
                      if l.startswith(publicador.CHAVE_SERPRO + "=")]) == 1,
                 "escrever de novo não duplica a chave")

        # O proxy muda de lugar entre distribuições, e uma máquina que mudou de
        # distribuição tem de trocar o valor, não ganhar uma segunda linha.
        outro = "/usr/lib/x86_64-linux-gnu/p11-kit-proxy.so"
        publicador.escrever_na_lista_serpro(outro)
        conferir(linhas().count("%s=%s" % (publicador.CHAVE_SERPRO, outro)) == 1
                 and len([l for l in linhas()
                          if l.startswith(publicador.CHAVE_SERPRO + "=")]) == 1,
                 "um caminho novo substitui o antigo")

        conferir(publicador.remover_da_lista_serpro(), "remover diz que removeu")
        conferir(linhas() == [DELES], "remover tira só a nossa linha")

        # Um arquivo que só tinha a nossa linha era nosso, e some junto. O
        # diretório também, e é o que devolve a máquina ao estado de quem nunca
        # publicou.
        with open(publicador.LISTA_SERPRO, "w", encoding="utf-8") as arquivo:
            arquivo.write("%s=%s\n" % (publicador.CHAVE_SERPRO, PROXY))
        publicador.remover_da_lista_serpro()
        conferir(not os.path.exists(publicador.LISTA_SERPRO)
                 and not os.path.isdir(os.path.dirname(publicador.LISTA_SERPRO)),
                 "o arquivo que só tinha a nossa linha some, e o diretório junto")

        return 1 if falhas else 0
    finally:
        shutil.rmtree(casa, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
