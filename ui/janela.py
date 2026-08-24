"""A janela: o que está instalado, o que dá para instalar, e se o token apareceu.

A tela responde, de cima para baixo, as perguntas na ordem em que a pessoa as
faz: "o meu certificado apareceu?" primeiro, "o que eu preciso instalar?"
depois. Quem abre isto está com o token na mão e quer assinar, não configurar.
"""
import subprocess
import threading

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
from gi.repository import Adw, GLib, Gtk  # noqa: E402

import catalogo  # noqa: E402
import instalador  # noqa: E402
import permissoes  # noqa: E402
import pkcs11  # noqa: E402
import publicador  # noqa: E402


class Janela(Adw.ApplicationWindow):
    def __init__(self, aplicacao):
        super().__init__(application=aplicacao, title="Certificado Digital")
        self.set_default_size(560, 640)

        cabecalho = Adw.HeaderBar()
        self.botao_atualizar = Gtk.Button(icon_name="view-refresh-symbolic",
                                          tooltip_text="Procurar o token de novo")
        # Reavalia também os componentes: a extensão instalada por comando,
        # fora da janela, só aparece aqui quando alguém pergunta de novo.
        self.botao_atualizar.connect("clicked", lambda _: self.atualizar_tudo())
        cabecalho.pack_end(self.botao_atualizar)

        self.toasts = Adw.ToastOverlay()
        pagina = Adw.PreferencesPage()

        # 1. O que a pessoa veio saber.
        self.grupo_tokens = Adw.PreferencesGroup(
            title="Seu certificado",
            description="O que este computador está enxergando agora")
        pagina.add(self.grupo_tokens)
        self.linhas_token = []

        # 2. O que falta para o token chegar aos programas dela.
        self.grupo_navegadores = Adw.PreferencesGroup(
            title="Navegadores e aplicativos",
            description=(
                "Publicar faz o seu certificado aparecer no Firefox, no Chrome, "
                "no Brave e no Papers que você já usa."))
        self.linha_publicar = Adw.ActionRow(title="Publicar para os navegadores")
        self.botao_publicar = Gtk.Button(valign=Gtk.Align.CENTER)
        self.botao_publicar.connect("clicked", self._clicou_publicar)
        self.linha_publicar.add_suffix(self.botao_publicar)
        self.grupo_navegadores.add(self.linha_publicar)
        pagina.add(self.grupo_navegadores)

        # 3. O que ela pode instalar.
        self.grupo_componentes = Adw.PreferencesGroup(
            title="Drivers de token",
            description=(
                "Instale o do seu token, se ele não aparecer acima. Baixados "
                "do site do fabricante, na sua máquina."))
        pagina.add(self.grupo_componentes)

        self.linhas = {}
        for componente in catalogo.por_tipo("driver"):
            self.linhas[componente.chave] = self._linha(componente)
            self.grupo_componentes.add(self.linhas[componente.chave]["linha"])

        # Assinadores em grupo próprio: são outra pergunta. Driver é "o meu
        # token aparece"; assinador é "este site consegue assinar".
        self.grupo_assinadores = Adw.PreferencesGroup(
            title="Assinadores",
            description=(
                "Para assinar dentro do navegador. Cada um precisa também da "
                "extensão correspondente, instalada no navegador."))
        for componente in catalogo.por_tipo("assinador"):
            self.linhas[componente.chave] = self._linha(componente)
            self.grupo_assinadores.add(self.linhas[componente.chave]["linha"])
        pagina.add(self.grupo_assinadores)

        # Aplicativos que chegam como extensão Flatpak. Grupo próprio porque a
        # instalação é diferente do resto da tela: não é um clique, é um
        # comando que a pessoa roda uma vez, e a janela precisa dizer isso sem
        # parecer que o botão quebrou.
        self.grupo_aplicativos = Adw.PreferencesGroup(
            title="Aplicativos",
            description=(
                "Programas que usam o mesmo token. Vêm à parte porque são "
                "grandes, e só quem precisa deles baixa."))
        for componente in catalogo.por_tipo("aplicativo"):
            self.linhas[componente.chave] = self._linha(componente)
            self.grupo_aplicativos.add(self.linhas[componente.chave]["linha"])
        pagina.add(self.grupo_aplicativos)

        rolagem = Gtk.ScrolledWindow(vexpand=True)
        rolagem.set_child(pagina)
        caixa = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        caixa.append(cabecalho)
        caixa.append(rolagem)
        self.toasts.set_child(caixa)
        self.set_content(self.toasts)

        self.atualizar_componentes()
        self.atualizar_publicacao()
        self.atualizar_tokens()

    # ------------------------------------------------------------------
    def _linha(self, componente):
        linha = Adw.ActionRow(title=componente.nome, subtitle=componente.resumo)

        botao = Gtk.Button(valign=Gtk.Align.CENTER)
        botao.connect("clicked", self._clicou, componente)

        # Só para o componente que traz um aplicativo com janela própria, como
        # o SerproID. Fica escondido enquanto ele não estiver instalado.
        abrir = Gtk.Button(label="Abrir", valign=Gtk.Align.CENTER, visible=False)
        abrir.connect("clicked", self._abrir, componente)
        linha.add_suffix(abrir)

        progresso = Gtk.ProgressBar(valign=Gtk.Align.CENTER, visible=False,
                                    show_text=True, width_request=140)

        linha.add_suffix(progresso)
        linha.add_suffix(botao)
        return {"linha": linha, "botao": botao, "progresso": progresso,
                "abrir": abrir}

    def atualizar_componentes(self):
        for componente in catalogo.CATALOGO:
            if componente.chave not in self.linhas:
                continue
            partes = self.linhas[componente.chave]
            posto = instalador.instalado(componente)
            if componente.extensao:
                # As reticências são a convenção de que o botão abre um
                # diálogo em vez de fazer a coisa.
                partes["botao"].set_label("Remover…" if posto else "Instalar…")
            else:
                partes["botao"].set_label("Remover" if posto else "Instalar")
            partes["botao"].set_sensitive(True)
            partes["botao"].set_css_classes(["destructive-action"] if posto
                                            else ["suggested-action"])
            # O tamanho é o do download, não o do que fica em disco: o
            # SafeNet baixa 91 MB e instala 3. Quem está numa conexão medida
            # precisa saber do primeiro número antes de tocar no botão.
            if posto:
                sufixo = " · instalado"
            elif componente.extensao:
                # Aqui o tamanho é o da extensão inteira, com a JVM dentro, e
                # não há barra de progresso para acompanhar: quem baixa é o
                # Flatpak, no terminal.
                sufixo = " · %d MB, instala por comando" % (
                    componente.tamanho // (1024 * 1024))
            elif componente.tamanho:
                sufixo = " · %d MB para baixar" % (
                    componente.tamanho // (1024 * 1024))
            else:
                sufixo = ""
            partes["linha"].set_subtitle(componente.resumo + sufixo)
            partes["abrir"].set_visible(
                bool(posto and instalador.lancador(componente)))

    def atualizar_tudo(self):
        self.atualizar_componentes()
        self.atualizar_publicacao()
        self.atualizar_tokens()

    def _abrir(self, _botao, componente):
        caminho = instalador.lancador(componente)
        if not caminho:
            return
        # Sem esperar: o aplicativo do componente tem janela própria e vida
        # própria, e travar a interface até ele fechar seria pior que não abrir.
        subprocess.Popen([caminho], start_new_session=True)

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
    def atualizar_publicacao(self):
        posto = publicador.publicado()
        self.botao_publicar.set_label("Despublicar" if posto else "Publicar")
        self.botao_publicar.set_css_classes(["destructive-action"] if posto
                                            else ["suggested-action"])
        self.linha_publicar.set_subtitle(
            "Publicado. Feche e reabra os navegadores." if posto
            else "Os navegadores ainda não enxergam o seu certificado.")

    def _clicou_publicar(self, botao):
        if publicador.publicado():
            resultado = publicador.despublicar()
            self.atualizar_publicacao()
            self.toasts.add_toast(Adw.Toast(
                title="Removido de %d lugar(es)" % len(resultado["modulos"])))
            return

        # A permissão é conferida na hora de usar, e não na abertura: ela pode
        # mudar entre uma coisa e outra, e um aviso mostrado cedo demais vira
        # ruído para quem nem ia publicar.
        pendencias = permissoes.faltando()
        if pendencias:
            self._pedir_permissao(pendencias)
            return

        resultado = publicador.publicar()
        self.atualizar_publicacao()

        if resultado["erros"]:
            self._avisar("Publiquei o que deu",
                         "\n".join(resultado["erros"]))
            return
        self.toasts.add_toast(Adw.Toast(
            title="Publicado. Feche e reabra os navegadores."))

    def _pedir_permissao(self, pendencias):
        """O diálogo que aparece quando o sandbox não alcança o que precisa.

        Ele não tenta consertar sozinho: um aplicativo não amplia as próprias
        permissões, e fingir que sim seria pior. O que ele faz é dizer o que
        falta, para que serve, e entregar o comando pronto para colar.
        """
        corpo = ["Este aplicativo roda numa caixa fechada e, para levar o seu "
                 "certificado aos navegadores, precisa escrever em alguns "
                 "lugares da sua pasta pessoal. Faltam:", ""]
        for _, para_que, _ in pendencias:
            corpo.append("   •  %s" % para_que)
        corpo += ["", "Cole isto num terminal e tente de novo:", "",
                  permissoes.comando(pendencias)]

        dialogo = Adw.MessageDialog(
            transient_for=self,
            heading="Falta permissão",
            body="\n".join(corpo))
        dialogo.add_response("fechar", "Fechar")
        dialogo.add_response("copiar", "Copiar comando")
        dialogo.set_response_appearance("copiar", Adw.ResponseAppearance.SUGGESTED)
        dialogo.connect("response", self._respondeu_permissao, pendencias)
        dialogo.present()

    def _respondeu_permissao(self, dialogo, resposta, pendencias):
        if resposta != "copiar":
            return
        self.get_clipboard().set(permissoes.comando(pendencias))
        self.toasts.add_toast(Adw.Toast(title="Comando copiado"))

    def _avisar(self, titulo, corpo):
        dialogo = Adw.MessageDialog(transient_for=self, heading=titulo, body=corpo)
        dialogo.add_response("fechar", "Fechar")
        dialogo.present()

    # ------------------------------------------------------------------
    def _mostrar_comando(self, componente, posto):
        """O diálogo do que a janela não pode fazer sozinha.

        Um aplicativo em sandbox não instala nem remove um Flatpak: fazer isso
        exigiria a permissão org.freedesktop.Flatpak, que é acesso irrestrito à
        máquina, e pedi-la para instalar uma extensão seria trocar um comando
        eventual por um risco permanente. Então a janela faz o que pode: diz
        exatamente qual é o comando.
        """
        if posto:
            comando = instalador.comando_de_desinstalar(componente)
            corpo = ["Para remover o %s, cole isto num terminal:" % componente.nome,
                     "", comando]
        else:
            comando = instalador.comando_de_instalar(componente)
            corpo = [componente.detalhe, "",
                     "Cole isto num terminal e volte aqui:", "", comando]
            # A permissão opcional aparece aqui, junto do resto, e não como um
            # segundo diálogo depois: quem está lendo comandos já está no
            # terminal. E aparece como opcional de verdade, com o que se perde
            # ao não dar: nada do fluxo pelo navegador depende dela.
            if componente.permissao and not permissoes.tem_documentos():
                argumento, para_que = componente.permissao
                corpo += ["", "Opcional, só se você for %s:" % para_que, "",
                          permissoes.comando_opcional(argumento)]

        dialogo = Adw.MessageDialog(
            transient_for=self,
            heading=("Remover o %s" if posto else "Instalar o %s") % componente.nome,
            body="\n".join(corpo))
        dialogo.add_response("fechar", "Fechar")
        dialogo.add_response("copiar", "Copiar comando")
        dialogo.set_response_appearance("copiar", Adw.ResponseAppearance.SUGGESTED)
        dialogo.connect("response", self._respondeu_comando, comando)
        dialogo.present()

    def _respondeu_comando(self, dialogo, resposta, comando):
        if resposta != "copiar":
            return
        self.get_clipboard().set(comando)
        self.toasts.add_toast(Adw.Toast(
            title="Comando copiado. Depois de rodá-lo, toque em atualizar."))

    def _clicou(self, botao, componente):
        if componente.extensao:
            self._mostrar_comando(componente, instalador.instalado(componente))
            return

        if instalador.instalado(componente):
            instalador.desinstalar(componente)
            self.atualizar_componentes()
            self._republicar()
            self.atualizar_publicacao()
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

    def _republicar(self):
        """Reescreve o que já estava publicado, depois de o catálogo mudar.

        Publicar de novo é o passo que se esquece, e o sintoma engana: quem
        instala um assinador vê "instalado" na janela, abre o navegador e a
        extensão continua dizendo que ele não existe, porque o manifesto que o
        navegador lê não menciona o que acabou de chegar. Vale nos dois
        sentidos, e é por isso que isto está aqui e não só na remoção.

        Só reescreve o que já estava publicado: publicar pela primeira vez é
        decisão de quem usa, e acontece pelo botão.
        """
        if publicador.publicado():
            publicador.publicar()

    def _terminou(self, componente):
        self.linhas[componente.chave]["progresso"].set_visible(False)
        self.atualizar_componentes()
        self._republicar()
        self.atualizar_publicacao()
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
