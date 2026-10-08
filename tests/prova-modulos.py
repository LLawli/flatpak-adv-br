"""Os .module publicados ficam de fora dos serviços da sessão gráfica.

Um .module sem restrição é carregado pelo gsd-smartcard e pelo gnome-shell na
subida da sessão, e cada módulo nosso vira um "flatpak run": no Fedora 44 com
GNOME isso travou o login, no Ubuntu 26.04 derrubou o gsd-smartcard a cada
8 segundos (relatos 14 e 15). O que se confere aqui:

  1. o arquivo sai com o disable-in, e republicar não o duplica;
  2. uma restrição que a pessoa escreveu à mão sobrevive à republicação;
  3. a lista do aplicativo e a da linha de comando são a mesma;
  4. o p11-kit de verdade esconde o módulo de um processo chamado
     gsd-smartcard e continua mostrando-o para os demais.
"""
import os
import re
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, "ui")
import pkcs11  # noqa: E402
import publicador  # noqa: E402

falhas = []

# Os processos que carregaram o driver sem ninguém pedir, medidos numa sessão
# GNOME do Fedora 44. Ver docs/ARMADILHAS.md.
MEDIDOS = ("gsd-smartcard", "gnome-software", "gvfsd-http")


def confere(condicao, mensagem):
    print(("  ok  " if condicao else "  FALHA ") + mensagem)
    if not condicao:
        falhas.append(mensagem)


def ler(caminho):
    with open(caminho, encoding="utf-8") as f:
        return f.read()


def publicar(drivers):
    pkcs11.modulos_instalados = lambda: drivers
    feito = {"modulos": [], "erros": []}
    publicador.escrever_modulos(feito)
    return feito


def lista_do_shell():
    texto = ler("host/comum.sh")
    achado = re.search(r'^FORA_DA_SESSAO="([^"]*)"', texto, re.M)
    return tuple(n.strip() for n in achado.group(1).split(",")) if achado else ()


def p11kit_lista(config, nome_do_programa, p11kit):
    """Os módulos que o p11-kit mostra a um processo com esse nome."""
    pasta = tempfile.mkdtemp(prefix="prova-modulos-bin.")
    try:
        programa = os.path.join(pasta, nome_do_programa)
        os.symlink(p11kit, programa)
        ambiente = dict(os.environ, HOME=os.path.dirname(config),
                        XDG_CONFIG_HOME=config)
        saida = subprocess.run([programa, "list-modules"], env=ambiente,
                               capture_output=True, text=True, timeout=30)
        return re.findall(r"^module: (\S+)", saida.stdout, re.M)
    finally:
        shutil.rmtree(pasta)


def main():
    casa = tempfile.mkdtemp(prefix="prova-modulos.")
    config = os.path.join(casa, ".config")
    os.environ["XDG_CONFIG_HOME"] = config
    try:
        publicar(["/app/lib/opensc-pkcs11.so"])
        arquivo = os.path.join(publicador.modulos_do_host(), "advbr-opensc-pkcs11.module")
        texto = ler(arquivo)
        esperada = "disable-in: " + ", ".join(publicador.FORA_DA_SESSAO)
        confere(esperada in texto.splitlines(), "o .module sai com o disable-in")
        # Os três que carregaram o driver na sessão GNOME medida.
        confere(set(MEDIDOS) <= set(publicador.FORA_DA_SESSAO),
                "a lista tem os serviços medidos")

        publicar(["/app/lib/opensc-pkcs11.so"])
        confere(ler(arquivo).count("disable-in:") == 1,
                "republicar não duplica a restrição")

        # A pessoa trocou a nossa linha pela dela, como fez quem relatou.
        editado = "\n".join(l for l in ler(arquivo).splitlines()
                            if not l.startswith("disable-in:")
                            and l != publicador.MARCA_RESTRICAO)
        with open(arquivo, "w", encoding="utf-8") as f:
            f.write(editado + "\nenable-in: firefox, p11-kit\n")
        publicar(["/app/lib/opensc-pkcs11.so"])
        depois = ler(arquivo).splitlines()
        confere("enable-in: firefox, p11-kit" in depois,
                "a restrição escrita à mão sobrevive")
        confere(not any(l.startswith("disable-in:") for l in depois),
                "e a nossa não entra junto, que o p11-kit recusaria as duas")

        confere(lista_do_shell() == publicador.FORA_DA_SESSAO,
                "host/comum.sh e ui/publicador.py têm a mesma lista")

        p11kit = shutil.which("p11-kit")
        remoto = next((c for c in ("/usr/lib/p11-kit/p11-kit-remote",
                                   "/usr/libexec/p11-kit/p11-kit-remote")
                       if os.access(c, os.X_OK)), None)
        confianca = next((c for c in ("/usr/lib/pkcs11/p11-kit-trust.so",
                                      "/usr/lib64/pkcs11/p11-kit-trust.so",
                                      "/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-trust.so")
                          if os.path.exists(c)), None)
        if not (p11kit and remoto and confianca):
            print("  --  sem p11-kit, p11-kit-remote ou p11-kit-trust.so aqui; "
                  "a prova com o p11-kit de verdade fica para outra máquina")
        else:
            # O mesmo arquivo que o aplicativo escreve, com um remote que de
            # fato responde no lugar do "flatpak run".
            os.unlink(arquivo)
            publicar(["/app/lib/opensc-pkcs11.so"])
            linhas = [l if not l.startswith("remote:")
                      else "remote: |%s %s" % (remoto, confianca)
                      for l in ler(arquivo).splitlines()]
            with open(arquivo, "w", encoding="utf-8") as f:
                f.write("\n".join(linhas) + "\n")

            nome = "advbr-opensc-pkcs11"
            confere(nome in p11kit_lista(config, "p11-kit", p11kit),
                    "o p11-kit mostra o módulo ao próprio p11-kit")
            for servico in MEDIDOS:
                confere(nome not in p11kit_lista(config, servico, p11kit),
                        "o p11-kit esconde o módulo do %s" % servico)
    finally:
        shutil.rmtree(casa)

    if falhas:
        sys.exit("%d falha(s)" % len(falhas))


main()
