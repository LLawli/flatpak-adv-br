"""A escala do monitor, descoberta pela janela e usada por quem não é GTK.

O PJeOffice é Swing rodando por XWayland, e o AWT do Java não descobre escala
fracionária sozinho: num monitor a 125% ou 150% a janela dele sai borrada, que
foi o que um usuário relatou. A JVM aceita o número pronto
(`-Dsun.java2d.uiScale`), mas alguém precisa saber qual é.

Quem sabe é a janela deste aplicativo, que é GTK e pergunta ao compositor. Ela
grava aqui, e os lançadores leem. O valor é o do monitor em que a janela estava
da última vez que foi aberta: com dois monitores de escalas diferentes, pode ser
o do outro. É a informação que dá para ter sem pedir nada a quem usa.
"""
import os

import registro

ARQUIVO = os.path.join(
    os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share"),
    "escala")


def gravar(valor):
    """Guarda a escala, se ela for um número plausível e tiver mudado."""
    try:
        valor = float(valor)
    except (TypeError, ValueError):
        return
    # Fora desta faixa é erro de leitura, não monitor exótico. Gravar um número
    # absurdo aqui sai como uma janela do PJeOffice do tamanho da tela, ou
    # invisível, e a causa estaria neste arquivo e não no Java.
    if not 0.5 <= valor <= 4:
        registro.registrar("escala fora da faixa (%s), ignorada", valor)
        return
    texto = ("%.3f" % valor).rstrip("0").rstrip(".")
    try:
        if ler() == texto:
            return
        os.makedirs(os.path.dirname(ARQUIVO), exist_ok=True)
        with open(ARQUIVO, "w", encoding="utf-8") as arquivo:
            arquivo.write(texto + "\n")
    except OSError as erro:
        registro.falha("não consegui gravar a escala", erro)


def detectar():
    """A escala dos monitores, perguntada ao compositor sem abrir janela.

    É o que permite abrir o PJeOffice direto pelo atalho do menu, sem passar
    pela janela deste aplicativo. Custa inicializar o GTK, alguns milissegundos
    antes de subir uma máquina virtual Java, o que é troco.

    Com mais de um monitor, devolve a MAIOR escala. Sem janela não há como saber
    em qual deles o assinador vai abrir (o Wayland não conta a posição do
    ponteiro para quem pergunta), e dos dois erros possíveis este é o menos
    ruim: escala de menos é a janela borrada que motivou tudo isto; escala de
    mais é uma janela grande, e legível.
    """
    try:
        import gi
        gi.require_version("Gtk", "4.0")
        gi.require_version("Gdk", "4.0")
        from gi.repository import Gdk, Gtk
    except (ImportError, ValueError) as erro:
        registro.falha("não deu para carregar o GTK para medir a escala", erro)
        return None

    if not Gtk.init_check():
        registro.registrar("sem display: a escala do monitor não foi medida")
        return None

    display = Gdk.Display.get_default()
    if display is None:
        return None
    monitores = display.get_monitors()
    valores = []
    for indice in range(monitores.get_n_items()):
        monitor = monitores.get_item(indice)
        # get_scale devolve a fracionária e existe do GTK 4.14 em diante;
        # get_scale_factor é a inteira, de sempre.
        obter = getattr(monitor, "get_scale", None)
        valor = obter() if obter is not None else 0
        valores.append(float(valor or monitor.get_scale_factor() or 1))
    return max(valores) if valores else None


def atualizar():
    """Mede e guarda. É o que o lançador de aplicativo chama."""
    valor = detectar()
    if valor:
        gravar(valor)


def ler():
    """A escala guardada, como texto, ou "" quando não há."""
    try:
        with open(ARQUIVO, encoding="utf-8") as arquivo:
            return arquivo.read().strip()
    except OSError:
        return ""
