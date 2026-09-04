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

        # O ícone. Quando o componente traz um, o Icon= tem de ser o CAMINHO
        # ABSOLUTO dele, e não o id do aplicativo: o atalho vive no menu do
        # host, que não tem tema de ícone nosso nem enxerga /app. O que os dois
        # lados enxergam igual é ~/.var/app/<id>/data.
        alvo = comaplicativo[0]
        pasta = os.path.join(instalador.diretorio(alvo), "icone")
        os.makedirs(pasta, exist_ok=True)
        svg = os.path.join(pasta, "dev.exemplo.Icone.svg")
        with open(svg, "w", encoding="utf-8") as arquivo:
            arquivo.write("<svg/>")
        instalador._escrever_atalho(alvo)
        texto = open(instalador._atalho(alvo), encoding="utf-8").read()
        if ("Icon=" + svg) in texto:
            print("  ok  %-10s usa o ícone que o componente trouxe" % alvo.chave)
        else:
            print("  ERRO %s: o ícone do componente não entrou no Icon="
                  % alvo.chave)
            falhas += 1
        shutil.rmtree(pasta, ignore_errors=True)

        # E sem ícone próprio, cai no do aplicativo, que é o que sempre foi.
        instalador._escrever_atalho(alvo)
        texto = open(instalador._atalho(alvo), encoding="utf-8").read()
        if ("Icon=%s\n" % catalogo.APP_ID) in texto:
            print("  ok  %-10s sem ícone próprio, usa o do aplicativo" % alvo.chave)
        else:
            print("  ERRO %s: sem ícone próprio, o Icon= ficou errado" % alvo.chave)
            falhas += 1
        instalador._remover_atalho(alvo)

        # Quando o componente traz o .desktop do próprio autor, ele é o que
        # vale: nome, descrição traduzida, palavras-chave e StartupWMClass são
        # metadados dele. Aqui só duas linhas podem mudar, porque só duas não
        # fazem sentido fora da máquina de quem escreveu: o Exec, que precisa
        # entrar pelo adv-br-aplicativo, e o Icon, que é um nome de tema que o
        # menu do host não resolve.
        pasta = os.path.join(instalador.diretorio(alvo), "atalhos")
        os.makedirs(pasta, exist_ok=True)
        with open(os.path.join(pasta, "x.desktop"), "w", encoding="utf-8") as arquivo:
            arquivo.write("[Desktop Entry]\nType=Application\nName=Do Autor\n"
                          "Comment[en]=From the author\nExec=programa-solto\n"
                          "Icon=dev.exemplo.Tema\nStartupWMClass=dev.exemplo.X\n")
        instalador._escrever_atalho(alvo)
        texto = open(instalador._atalho(alvo), encoding="utf-8").read()

        esperado = "Exec=flatpak run --command=adv-br-aplicativo %s %s\n" % (
            catalogo.APP_ID, alvo.chave)
        problemas = []
        if esperado not in texto:
            problemas.append("o Exec do autor não foi trocado pelo nosso")
        if "Icon=dev.exemplo.Tema" in texto:
            problemas.append("o Icon do autor sobreviveu, e o host não o resolve")
        for guardar in ("Name=Do Autor", "Comment[en]=From the author",
                        "StartupWMClass=dev.exemplo.X"):
            if guardar not in texto:
                problemas.append("perdeu o que era do autor: %s" % guardar)
        if "X-Flatpak=%s" % catalogo.APP_ID not in texto:
            problemas.append("não marcou o atalho como deste aplicativo")
        if problemas:
            for p in problemas:
                print("  ERRO %s: %s" % (alvo.chave, p))
            falhas += len(problemas)
        else:
            print("  ok  %-10s usa o .desktop do autor, trocando só Exec e Icon"
                  % alvo.chave)
        shutil.rmtree(pasta, ignore_errors=True)
        instalador._remover_atalho(alvo)

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
