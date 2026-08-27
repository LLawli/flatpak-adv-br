"""O atalho de menu dos componentes que trazem aplicativo.

O que este teste protege é o Exec. Enquanto o único jeito de abrir era o botão
da janela, o aplicativo do componente herdava dela o preparo dos drivers: o
LD_PRELOAD da libgcc_s que o SerproID exige, os diretórios que ele faz readdir
antes de existirem, a configuração do SafeNet copiada para /etc. Um atalho que
chamasse o lançador direto pularia tudo isso, e o sintoma seria o PJeOffice
abrindo sem enxergar um token que está instalado, sem erro nenhum na tela.
"""
import os
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, "ui")
import catalogo  # noqa: E402


def main():
    casa = tempfile.mkdtemp(prefix="prova-atalho.")
    anterior = os.environ.get("HOME")
    os.environ["HOME"] = casa
    try:
        import importlib
        import instalador
        importlib.reload(instalador)

        comaplicativo = [c for c in catalogo.CATALOGO if c.lancador]
        semaplicativo = [c for c in catalogo.CATALOGO if not c.lancador][:1]
        falhas = 0

        if not comaplicativo:
            print("  ERRO nenhum componente com aplicativo no catálogo")
            return 1

        for componente in comaplicativo:
            instalador._escrever_atalho(componente)
            caminho = instalador._atalho(componente)
            if not os.path.exists(caminho):
                print("  ERRO %s não gerou atalho" % componente.chave)
                falhas += 1
                continue
            texto = open(caminho, encoding="utf-8").read()

            esperado = ("Exec=flatpak run --command=adv-br-aplicativo %s %s"
                        % (catalogo.APP_ID, componente.chave))
            if esperado not in texto:
                print("  ERRO %s: o Exec não entra pelo adv-br-aplicativo"
                      % componente.chave)
                falhas += 1
            elif "/componentes/" in texto:
                print("  ERRO %s: o Exec chama o lançador direto e pula o preparo"
                      % componente.chave)
                falhas += 1
            else:
                print("  ok  %-10s abre pelo preparo comum" % componente.chave)

            if shutil.which("desktop-file-validate"):
                r = subprocess.run(["desktop-file-validate", caminho],
                                   capture_output=True, text=True)
                if r.returncode or (r.stdout + r.stderr).strip():
                    print("  ERRO %s: %s" % (componente.chave,
                                             (r.stdout + r.stderr).strip()))
                    falhas += 1
                else:
                    print("  ok  %-10s aprovado pelo desktop-file-validate"
                          % componente.chave)

            instalador._remover_atalho(componente)
            if os.path.exists(caminho):
                print("  ERRO %s: o atalho sobrou depois de desinstalar"
                      % componente.chave)
                falhas += 1
            else:
                print("  ok  %-10s some ao desinstalar" % componente.chave)

        # Componente sem aplicativo não pode virar atalho: um driver não abre.
        for componente in semaplicativo:
            instalador._escrever_atalho(componente)
            if os.path.exists(instalador._atalho(componente)):
                print("  ERRO %s não tem aplicativo e ganhou atalho" % componente.chave)
                falhas += 1
            else:
                print("  ok  %-10s não vira atalho, por não ter aplicativo"
                      % componente.chave)

        return 1 if falhas else 0
    finally:
        if anterior is not None:
            os.environ["HOME"] = anterior
        shutil.rmtree(casa, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
