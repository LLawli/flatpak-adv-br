"""O relato leva o diagnóstico do próprio RemoteID, enquanto ele está em teste.

Os dois primeiros relatos de quem foi usar o RemoteID chegaram inconclusivos: o
`app-remoteid.log` trazia avisos do GTK e nada mais, e o que aconteceu entre o
módulo, o aplicativo e a nuvem da Certisign não estava em lugar nenhum. Sem
isto, a resposta a quem relata é "não deu para saber".

É uma exceção deliberada e datada — o resto do projeto mantém esse arquivo fora
do relato, porque ele identifica o titular do certificado. O que este teste
guarda são as propriedades que a tornam aceitável: que o conteúdo entra, que a
execução mais recente vem primeiro, que há teto de tamanho, e que a sanitização
passa por ele como passa pelo resto.
"""
import json
import os
import shutil
import sys
import tempfile

RAIZ = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(RAIZ, "ui"))

import diagnostico  # noqa: E402
import sanitizar  # noqa: E402

CASAS = os.path.join(os.environ.get("XDG_CACHE_HOME")
                     or os.path.expanduser("~/.cache"), "adv-br-testes")


def _diag(pasta, execucoes):
    """Monta um diretório de diagnóstico do RemoteID e o aponta no ambiente."""
    os.makedirs(pasta, exist_ok=True)
    for i, corpo in enumerate(execucoes):
        caminho = os.path.join(pasta, "run-%d-1.jsonl" % (1000 + i))
        with open(caminho, "w", encoding="utf-8") as arquivo:
            arquivo.write(corpo)
        # A ordem é por data de modificação, e a mais nova vem primeiro.
        os.utime(caminho, (1000 + i, 1000 + i))
    os.environ["REMOTEID_DIAG_DIR"] = pasta
    return pasta


def conferir():
    problemas = []
    os.makedirs(CASAS, exist_ok=True)
    raiz = tempfile.mkdtemp(prefix="prova-relato-", dir=CASAS)
    salvo = os.environ.get("REMOTEID_DIAG_DIR")
    try:
        # 1. O conteúdo entra, e a execução mais recente vem primeiro: é a que
        #    tem a falha que a pessoa acabou de relatar.
        _diag(os.path.join(raiz, "a"), [
            json.dumps({"evento": "sessao.inicio", "ts": "antiga"}) + "\n",
            json.dumps({"evento": "http.request", "ts": "recente"}) + "\n",
        ])
        texto = diagnostico._remoteid()
        if "recente" not in texto or "antiga" not in texto:
            problemas.append("as execuções não entraram no relato")
        elif texto.index("recente") > texto.index("antiga"):
            problemas.append("a execução mais recente não veio primeiro")

        # 2. Um diretório sem execução nenhuma não vira uma seção vazia no meio
        #    do relato.
        vazio = os.path.join(raiz, "vazio")
        os.makedirs(vazio, exist_ok=True)
        os.environ["REMOTEID_DIAG_DIR"] = vazio
        if diagnostico._remoteid() != "":
            problemas.append("sem execução gravada, ainda assim escreveu algo")

        # 3. O teto de tamanho existe e é respeitado. Uma issue não pode virar
        #    um despejo de megabytes, e o RemoteID guarda 20 execuções.
        gorda = json.dumps({"evento": "x", "lixo": "a" * 20000}) + "\n"
        _diag(os.path.join(raiz, "grande"), [gorda] * 10)
        texto = diagnostico._remoteid()
        if len(texto) > diagnostico.BYTES_DO_REMOTEID + 4096:
            problemas.append("passou do teto: %d bytes" % len(texto))
        if "ficaram de fora" not in texto:
            problemas.append("cortou por tamanho sem dizer que cortou")

        # 4. E a sanitização morde o que passa por aqui. O RemoteID já redige
        #    senha, PIN e OTP; o CPF do titular é conosco.
        _diag(os.path.join(raiz, "cpf"), [
            json.dumps({"evento": "login", "cpf": "12345678901"}) + "\n"])
        limpo = sanitizar.sanitizar(diagnostico._remoteid())
        if "12345678901" in limpo:
            problemas.append("o CPF do titular sobreviveu à sanitização")
    finally:
        if salvo is None:
            os.environ.pop("REMOTEID_DIAG_DIR", None)
        else:
            os.environ["REMOTEID_DIAG_DIR"] = salvo
        shutil.rmtree(raiz, ignore_errors=True)
    return problemas


def main():
    problemas = conferir()
    for problema in problemas:
        print("  " + problema, file=sys.stderr)
    if not problemas:
        print("  ok  o relato leva o diagnóstico do RemoteID, com teto e sanitizado")
    return 1 if problemas else 0


if __name__ == "__main__":
    sys.exit(main())
