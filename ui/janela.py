"""A janela: o que está instalado, o que dá para instalar, e se o token apareceu.

A tela responde, de cima para baixo, as perguntas na ordem em que a pessoa as
faz: "o meu certificado apareceu?" primeiro, "o que eu preciso instalar?"
depois. Quem abre isto está com o token na mão e quer assinar, não configurar.
"""
import threading

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402

import catalogo  # noqa: E402
import instalador  # noqa: E402
import pkcs11  # noqa: E402


class Janela(Adw.ApplicationWindow):
    def __init__(self, aplicacao):
        super().__init__(application=aplicacao, title="Certificado Digital")
        self.set_default_size(560, 640)

        cabecalho = Adw.HeaderBar()
        self.botao_atualizar = Gtk.Button(icon_name="view-refresh-symbolic",
                                          tooltip_text="Procurar o token de novo")
        self.botao_atualizar.connect("clicked", lambda _: self.atualizar_tokens())
        cabecalho.pack_end(self.botao_atualizar)

        self.toasts = Adw.ToastOverlay()
        pagina = Adw.PreferencesPage()

        # 1. O que a pessoa veio saber.
        self.grupo_tokens = Adw.PreferencesGroup(
            title="Seu certificado",
            description="O que este computador está enxergando agora")
        pagina.add(self.grupo_tokens)
        self.linhas_token = []

        # 2. O que ela pode fazer a respeito.
        self.grupo_componentes = Adw.PreferencesGroup(
            title="Drivers e assinadores",
            description=(
                "Baixados do site do fabricante, na sua máquina. "
                "Instale só o que você usa."))
        pagina.add(self.grupo_componentes)

        self.linhas = {}
        for componente in catalogo.CATALOGO:
            self.linhas[componente.chave] = self._linha(componente)
            self.grupo_componentes.add(self.linhas[componente.chave]["linha"])

        rolagem = Gtk.ScrolledWindow(vexpand=True)
        rolagem.set_child(pagina)
        caixa = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        caixa.append(cabecalho)
        caixa.append(rolagem)
        self.toasts.set_child(caixa)
        self.set_content(self.toasts)

        self.atualizar_componentes()
        self.atualizar_tokens()

    # ------------------------------------------------------------------
    def _linha(self, componente):
        linha = Adw.ActionRow(title=componente.nome, subtitle=componente.resumo)

        botao = Gtk.Button(valign=Gtk.Align.CENTER)
        botao.connect("clicked", self._clicou, componente)

        progresso = Gtk.ProgressBar(valign=Gtk.Align.CENTER, visible=False,
                                    show_text=True, width_request=140)

        linha.add_suffix(progresso)
        linha.add_suffix(botao)
        return {"linha": linha, "botao": botao, "progresso": progresso}

    def atualizar_componentes(self):
        for componente in catalogo.CATALOGO:
            partes = self.linhas[componente.chave]
            posto = instalador.instalado(componente)
            partes["botao"].set_label("Remover" if posto else "Instalar")
            partes["botao"].set_sensitive(True)
            partes["botao"].set_css_classes(["destructive-action"] if posto
                                            else ["suggested-action"])
            partes["linha"].set_subtitle(
                componente.resumo + (" · instalado" if posto else ""))

    def atualizar_tokens(self):
        pkcs11.registrar()
        for linha in self.linhas_token:
            self.grupo_tokens.remove(linha)
        self.linhas_token = []

        encontrados = pkcs11.tokens()
        if not encontrados:
            linha = Adw.ActionRow(
                title="Nenhum certificado encontrado",
                subtitle=("Espete o token e toque no botão de atualizar. "
                          "Se ele continuar sem aparecer, instale o driver "
                          "correspondente abaixo."))
            linha.add_prefix(Gtk.Image(icon_name="dialog-information-symbolic"))
            self.grupo_tokens.add(linha)
            self.linhas_token.append(linha)
            return

        for token in encontrados:
            linha = Adw.ActionRow(
                title=token["rotulo"] or "(sem nome)",
                subtitle="%s · %s" % (token["fabricante"], token["modelo"]))
            linha.add_prefix(Gtk.Image(icon_name="emblem-ok-symbolic"))
            self.grupo_tokens.add(linha)
            self.linhas_token.append(linha)

    # ------------------------------------------------------------------
    def _clicou(self, botao, componente):
        if instalador.instalado(componente):
            instalador.desinstalar(componente)
            self.atualizar_componentes()
            self.atualizar_tokens()
            self.toasts.add_toast(Adw.Toast(title="%s removido" % componente.nome))
            return

        partes = self.linhas[componente.chave]
        botao.set_sensitive(False)
        botao.set_label("Instalando…")
        partes["progresso"].set_visible(True)
        partes["progresso"].set_fraction(0)

        # O download roda fora da thread da interface; sem isso a janela
        # congela e o usuário conclui que travou.
        threading.Thread(target=self._instalar, args=(componente,),
                         daemon=True).start()

    def _instalar(self, componente):
        partes = self.linhas[componente.chave]

        def progresso(recebido, total):
            fracao = recebido / total if total else 0.0
            GLib.idle_add(partes["progresso"].set_fraction, fracao)
            GLib.idle_add(partes["progresso"].set_text,
                          "%d %%" % int(fracao * 100) if total else "baixando")

        try:
            instalador.instalar(componente, progresso)
        except Exception as erro:  # noqa: BLE001
            GLib.idle_add(self._falhou, componente, str(erro))
            return
        GLib.idle_add(self._terminou, componente)

    def _terminou(self, componente):
        self.linhas[componente.chave]["progresso"].set_visible(False)
        self.atualizar_componentes()
        self.atualizar_tokens()
        self.toasts.add_toast(Adw.Toast(title="%s instalado" % componente.nome))

    def _falhou(self, componente, mensagem):
        partes = self.linhas[componente.chave]
        partes["progresso"].set_visible(False)
        self.atualizar_componentes()

        dialogo = Adw.MessageDialog(
            transient_for=self,
            heading="Não consegui instalar o %s" % componente.nome,
            body=mensagem)
        dialogo.add_response("fechar", "Fechar")
        dialogo.present()
