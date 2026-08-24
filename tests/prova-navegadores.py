"""Descoberta de navegadores, contra uma casa de mentira.

A lista fixa de nomes que isto substituiu atendia Firefox, Chrome, Chromium,
Brave, Vivaldi, Edge e Opera, e ignorava em silêncio quem usa qualquer outro.
Os casos abaixo são justamente os que ela não cobria, e o teste existe para que
voltar à lista fixa seja uma decisão, e não um descuido.
"""
import os
import shutil
import sys
import tempfile

sys.path.insert(0, "ui")
import publicador  # noqa: E402


def montar(casa, caminho, familia):
    """Cria os marcadores que aquela família de navegador deixa sozinha."""
    destino = os.path.join(casa, caminho)
    os.makedirs(destino, exist_ok=True)
    if familia == "firefox":
        open(os.path.join(destino, "profiles.ini"), "w").close()
    else:
        open(os.path.join(destino, "Local State"), "w").close()
        os.makedirs(os.path.join(destino, "Default"), exist_ok=True)


def main():
    casa = tempfile.mkdtemp(prefix="prova-navegadores.")
    try:
        # (onde o navegador guarda o perfil, família, onde ele lê os
        #  manifestos de native messaging)
        casos = [
            (".mozilla/firefox", "firefox", ".mozilla/native-messaging-hosts"),
            (".librewolf", "firefox", ".librewolf/native-messaging-hosts"),
            (".zen", "firefox", ".zen/native-messaging-hosts"),
            (".floorp", "firefox", ".floorp/native-messaging-hosts"),
            (".config/chromium", "chromium", ".config/chromium/NativeMessagingHosts"),
            (".config/BraveSoftware/Brave-Browser", "chromium",
             ".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"),
            (".config/vivaldi", "chromium", ".config/vivaldi/NativeMessagingHosts"),
        ]
        for caminho, familia, _ in casos:
            montar(casa, caminho, familia)

        # Ruído que NÃO é navegador e não pode ser tratado como um. O Electron
        # deixa "Local State" igualzinho ao Chromium; o que o separa é não ter
        # perfil de navegador.
        #
        # A casa deste aplicativo entra no mesmo saco, e é o caso que mais
        # enganou: ele tem --filesystem para o config dos navegadores, e o
        # Flatpak monta cada um deles também dentro do config dele. Sem excluir
        # a própria casa, o navegador do host aparece uma segunda vez como se
        # fosse um Flatpak, e a segunda passagem estraga o que a primeira
        # escreveu.
        os.makedirs(os.path.join(casa, ".config/algum-electron"), exist_ok=True)
        open(os.path.join(casa, ".config/algum-electron/Local State"), "w").close()
        os.makedirs(os.path.join(casa, "Documentos/projeto"), exist_ok=True)
        open(os.path.join(casa, "Documentos/projeto/profiles.ini"), "w").close()

        achados = {os.path.relpath(c, casa): f
                   for c, f in publicador.navegadores(casa)}
        falhas = 0

        for caminho, familia, manifestos in casos:
            if achados.get(caminho) != familia:
                print("  ERRO não achou %s como %s" % (caminho, familia))
                falhas += 1
                continue
            obtido = os.path.relpath(
                publicador.native_messaging(os.path.join(casa, caminho), familia),
                casa)
            if obtido != manifestos:
                print("  ERRO %s: manifestos em %s, esperado %s"
                      % (caminho, obtido, manifestos))
                falhas += 1
                continue
            print("  ok  %-38s %-9s -> %s" % (caminho, familia, manifestos))

        for intruso in (".config/algum-electron", "Documentos/projeto"):
            if intruso in achados:
                print("  ERRO tratou %s como navegador" % intruso)
                falhas += 1
            else:
                print("  ok  ignorou %s" % intruso)

        proprio = os.path.join(casa, ".var", "app", publicador.APP_ID)
        montar(proprio, "config/BraveSoftware/Brave-Browser", "chromium")
        casas = [c for c, _ in publicador._casas()]
        # _casas() olha o home de verdade, então o que se confere aqui é a
        # regra, não o resultado: nenhuma casa pode ser a deste aplicativo.
        if any(os.path.basename(c) == publicador.APP_ID for c in casas):
            print("  ERRO _casas() inclui a casa do próprio aplicativo")
            falhas += 1
        else:
            print("  ok  ignorou a casa do próprio aplicativo")

        return 1 if falhas else 0
    finally:
        shutil.rmtree(casa, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
