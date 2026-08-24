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
import diagnostico  # noqa: E402
import publicador  # noqa: E402
import registro  # noqa: E402
import relator  # noqa: E402
import sanitizar  # noqa: E402
import serie  # noqa: E402


def decidir(desenhado, real):
    """O que fazer ao clicar, dado o que o botão mostrava e o que o disco diz.

    Os dois podem divergir, e divergir é o caso interessante: a janela pode
    estar aberta desde antes de algo mudar por fora (outra janela do mesmo
    aplicativo, um `rm` no diretório de dados, uma instalação que terminou em
    outro lugar). Quando isso acontece, agir pelo disco faz o botão executar o
    OPOSTO do que ele diz, que foi como um clique em "Remover" acabou
    reinstalando o PJeOffice.

    Fazer o que o rótulo diz também não serve: seria remover o que não existe,
    ou baixar de novo o que já está lá. A saída é não agir e sincronizar, que é
    o único desfecho que não surpreende quem clicou.
    """
    if desenhado is not None and desenhado != real:
        return "sincronizar"
    return "remover" if real else "instalar"


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
        self.linhas = {}

        # 0. O aviso que precede tudo, quando existe: sem a ponte falando a
        # mesma língua do sistema, o certificado aparece e nada assina. Fica
        # acima de tudo porque é a única coisa aqui que faz o resto ser mentira.
        self.grupo_serie = Adw.PreferencesGroup(
            title="Antes de mais nada",
            visible=False)
        for componente in catalogo.por_tipo("compatibilidade"):
            self.linhas[componente.chave] = self._linha(componente)
            self.grupo_serie.add(self.linhas[componente.chave]["linha"])
        pagina.add(self.grupo_serie)

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

        # Por último, porque é o que se procura quando o resto não resolveu.
        self.grupo_relato = Adw.PreferencesGroup(
            title="Algo não funcionou?",
            description="Manda o que está acontecendo para quem cuida do "
                        "aplicativo, junto de um resumo técnico deste "
                        "computador. Você vê tudo antes de enviar.")
        linha_relato = Adw.ActionRow(title="Relatar um problema")
        botao_relato = Gtk.Button(label="Relatar", valign=Gtk.Align.CENTER)
        botao_relato.connect("clicked", self._clicou_relatar)
        linha_relato.add_suffix(botao_relato)
        self.grupo_relato.add(linha_relato)
        pagina.add(self.grupo_relato)

        # Estado do diálogo de relato, quando houver um aberto.
        self.relato_dialogo = None
        self.relato_desafio = None
        self.relato_nonce = None
        self.relato_cancelado = False
        self.relato_texto = None
        self.relato_diagnostico = None

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
        self.atualizar_serie()

    def atualizar_serie(self):
        """Mostra o aviso de compatibilidade, quando ele for verdade.

        Só aparece quando as duas séries foram lidas e divergem. Não conseguir
        ler é diferente de estar errado: nesse caso a janela fica como estava,
        porque um aviso baseado em leitura falha custaria mais confiança do que
        a ausência dele.
        """
        pendencia = serie.pendencia()
        for componente in catalogo.por_tipo("compatibilidade"):
            self.linhas[componente.chave]["linha"].set_visible(False)

        if pendencia is None:
            self.grupo_serie.set_visible(False)
            return

        serie_host, componente = pendencia
        if componente is None:
            # Divergência sem componente que resolva: dizer o que se sabe é
            # melhor que deixar a pessoa descobrir na hora de assinar.
            self.grupo_serie.set_visible(False)
            self.toasts.add_toast(Adw.Toast(
                title="O seu sistema usa o p11-kit %s, que este aplicativo "
                      "ainda não acompanha." % serie_host))
            return

        self.linhas[componente.chave]["linha"].set_visible(True)
        self.grupo_serie.set_visible(True)

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
        # "posto" guarda o que o botão mostra, para o clique poder comparar
        # com o que o disco diz na hora. Ver decidir().
        return {"linha": linha, "botao": botao, "progresso": progresso,
                "abrir": abrir, "posto": None}

    def atualizar_componentes(self):
        for componente in catalogo.CATALOGO:
            if componente.chave not in self.linhas:
                continue
            partes = self.linhas[componente.chave]
            posto = instalador.instalado(componente)
            partes["posto"] = posto
            partes["botao"].set_label("Remover" if posto else "Instalar")
            partes["botao"].set_sensitive(True)
            partes["botao"].set_css_classes(["destructive-action"] if posto
                                            else ["suggested-action"])
            # O tamanho é o do download, não o do que fica em disco: o
            # SafeNet baixa 91 MB e instala 3. Quem está numa conexão medida
            # precisa saber do primeiro número antes de tocar no botão.
            if posto:
                sufixo = " · instalado"
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
        self.atualizar_serie()

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
        self.publicacao_desenhada = posto
        self.botao_publicar.set_label("Despublicar" if posto else "Publicar")
        self.botao_publicar.set_css_classes(["destructive-action"] if posto
                                            else ["suggested-action"])
        self.linha_publicar.set_subtitle(
            "Publicado. Feche e reabra os navegadores." if posto
            else "Os navegadores ainda não enxergam o seu certificado.")

    def _clicou_publicar(self, botao):
        # Ver decidir(): o botão de publicar sofre da mesma divergência que o
        # dos componentes. Com a janela aberta desde antes de a publicação
        # mudar por fora, "Publicar" despublicava, e o pior é que isso parecia
        # ter funcionado: o toast some, o popup das permissões não aparece
        # porque não houve publicação, e fica tudo com cara de silêncio.
        acao = decidir(getattr(self, "publicacao_desenhada", None),
                       publicador.publicado())
        if acao == "sincronizar":
            self.atualizar_publicacao()
            self.toasts.add_toast(Adw.Toast(
                title="A publicação mudou por fora desta janela. Confira e "
                      "clique de novo."))
            return

        if acao == "remover":
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

        pedidos = permissoes.comandos_de_navegador(
            resultado["sandbox_client"], resultado["sandbox_assinador"])
        if pedidos:
            self._pedir_permissao_de_navegador(pedidos)
            return

        self.toasts.add_toast(Adw.Toast(
            title="Publicado. Feche e reabra os navegadores."))

    def _dialogo_de_comandos(self, titulo, paragrafos, comandos, rodape=(),
                             confirmar="Copiar comando"):
        """Diálogo que entrega comandos para colar, legível.

        O corpo de um Adw.MessageDialog é um parágrafo só, centralizado e com
        largura estreita. Um comando como

            flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 org...

        não cabe nessa largura, e o diálogo cresce para baixo quebrando a linha
        em qualquer lugar, até sair da tela. O que se lê então é um bloco alto e
        estreito de fragmentos de comando, que ninguém consegue conferir antes
        de colar num terminal.

        Aqui o texto fica no corpo e os comandos vão num filho à parte: cada um
        em fonte monoespaçada, numa linha só, selecionável, com rolagem
        horizontal própria. O diálogo tem largura mínima suficiente para uma
        linha de comando de tamanho normal, e a lista inteira rola na vertical
        se for longa.
        """
        caixa = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=6)
        # Largura pensada para o comando mais longo que este aplicativo
        # mostra (o override do socket do p11-kit, com o id do navegador no
        # fim). O que passar disso rola na horizontal em vez de quebrar.
        caixa.set_size_request(620, -1)

        for comando in comandos:
            rotulo = Gtk.Label(label=comando, xalign=0, selectable=True)
            rotulo.add_css_class("monospace")
            rolagem = Gtk.ScrolledWindow(
                hscrollbar_policy=Gtk.PolicyType.AUTOMATIC,
                vscrollbar_policy=Gtk.PolicyType.NEVER,
                propagate_natural_width=False)
            rolagem.set_child(rotulo)
            moldura = Gtk.Frame()
            moldura.add_css_class("view")
            moldura.set_child(rolagem)
            caixa.append(moldura)

        for texto in rodape:
            nota = Gtk.Label(label=texto, xalign=0, wrap=True)
            nota.add_css_class("dim-label")
            nota.add_css_class("caption")
            caixa.append(nota)

        # Se houver muito comando, a lista rola em vez de empurrar o diálogo
        # para fora da tela.
        externa = Gtk.ScrolledWindow(
            hscrollbar_policy=Gtk.PolicyType.NEVER,
            vscrollbar_policy=Gtk.PolicyType.AUTOMATIC,
            propagate_natural_height=True, max_content_height=320)
        externa.set_child(caixa)

        dialogo = Adw.MessageDialog(
            transient_for=self,
            heading=titulo,
            body="\n\n".join(paragrafos))
        dialogo.set_extra_child(externa)
        dialogo.add_response("fechar", "Fechar")
        dialogo.add_response("copiar", confirmar)
        dialogo.set_response_appearance("copiar", Adw.ResponseAppearance.SUGGESTED)
        dialogo.connect("response", self._respondeu_comando,
                        "\n".join(comandos))
        dialogo.present()
        return dialogo

    def _pedir_permissao_de_navegador(self, pedidos):
        """O que falta do lado do navegador em Flatpak, que só ele pode dar.

        Sem isto a publicação parece ter dado certo e nada funciona: o
        navegador em sandbox não lê módulo PKCS#11 de usuário e não pode
        executar o assinador. Era preciso ler o repositório do projeto para
        descobrir quais permissões faltavam, o que é pedir demais de quem só
        quer assinar uma petição.
        """
        paragrafos = ["Publiquei tudo. Os navegadores que rodam em Flatpak "
                      "precisam de uma permissão a mais, que só você pode dar:"]
        paragrafos.append("\n".join("•  %s" % para_que
                                    for para_que, _ in pedidos))
        paragrafos.append("Cole no terminal:")

        # Nem todo mundo abre terminal, e as permissões de um Flatpak têm
        # editor gráfico: o Flatseal, e a aba de permissões do próprio
        # gerenciador de aplicativos. Só a linha do systemctl não tem
        # equivalente, e é honesto dizer qual é qual em vez de mandar a pessoa
        # procurar tudo num lugar onde metade não está.
        rodape = []
        if any(c.startswith("flatpak override") for _, c in pedidos):
            rodape.append("As linhas de permissão também dão para fazer sem "
                          "terminal, pelo Flatseal ou pela aba de permissões "
                          "do seu gerenciador de aplicativos. Procure o "
                          "navegador na lista.")
        if any(c.startswith("systemctl") for _, c in pedidos):
            rodape.append("A linha do systemctl não tem equivalente gráfico: "
                          "ela liga um serviço do seu sistema.")
        rodape.append("Se você já fez isto antes, pode fechar. Depois, feche e "
                      "reabra os navegadores.")

        self._dialogo_de_comandos(
            "Falta permissão nos navegadores", paragrafos,
            [comando for _, comando in pedidos], rodape,
            confirmar="Copiar comandos")

    def _pedir_permissao(self, pendencias):
        """O diálogo que aparece quando o sandbox não alcança o que precisa.

        Ele não tenta consertar sozinho: um aplicativo não amplia as próprias
        permissões, e fingir que sim seria pior. O que ele faz é dizer o que
        falta, para que serve, e entregar o comando pronto para colar.
        """
        paragrafos = ["Este aplicativo roda numa caixa fechada e, para levar "
                      "o seu certificado aos navegadores, precisa escrever em "
                      "alguns lugares da sua pasta pessoal. Faltam:"]
        paragrafos.append("\n".join("•  %s" % para_que
                                    for _, para_que, _ in pendencias))
        paragrafos.append("Cole no terminal e tente de novo:")

        self._dialogo_de_comandos(
            "Falta permissão", paragrafos, [permissoes.comando(pendencias)])

    def _respondeu_comando(self, dialogo, resposta, comando):
        """Resposta de qualquer diálogo que entrega comandos para colar."""
        if resposta != "copiar":
            return
        self.get_clipboard().set(comando)
        self.toasts.add_toast(Adw.Toast(
            title="Copiado. Depois de rodar, toque em atualizar."))

    def _avisar(self, titulo, corpo):
        dialogo = Adw.MessageDialog(transient_for=self, heading=titulo, body=corpo)
        dialogo.add_response("fechar", "Fechar")
        dialogo.present()

    # ------------------------------------------------------------------
    def _clicou_relatar(self, _botao):
        """O diálogo de relatar um problema.

        Três coisas acontecem ao abrir, e duas delas fora da thread da
        interface: a prova de trabalho começa a ser resolvida (leva alguns
        segundos e ninguém precisa saber disso) e o diagnóstico é coletado. A
        terceira é a pessoa escrever o que aconteceu, que é o tempo que as
        outras duas têm para terminar.
        """
        self.relato_desafio = None
        self.relato_nonce = None
        self.relato_cancelado = False

        caixa = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=12)
        caixa.set_size_request(620, -1)

        self.relato_texto = Gtk.TextView(
            wrap_mode=Gtk.WrapMode.WORD_CHAR, top_margin=8, bottom_margin=8,
            left_margin=8, right_margin=8, accepts_tab=False)
        self.relato_texto.get_buffer().set_text("")
        rolagem = Gtk.ScrolledWindow(
            hscrollbar_policy=Gtk.PolicyType.NEVER, min_content_height=110,
            max_content_height=160, propagate_natural_height=True)
        rolagem.set_child(self.relato_texto)
        moldura = Gtk.Frame()
        moldura.add_css_class("view")
        moldura.set_child(rolagem)
        caixa.append(moldura)

        # A prévia mostra o texto EXATO que vai ser enviado, e é editável: quem
        # quiser tirar mais alguma coisa, tira. Fica recolhida porque é longa,
        # e aberta com um clique porque esconder o que se envia seria pedir
        # confiança em vez de dá-la.
        self.relato_diagnostico = Gtk.TextView(
            monospace=True, wrap_mode=Gtk.WrapMode.NONE, top_margin=8,
            bottom_margin=8, left_margin=8, right_margin=8)
        self.relato_diagnostico.get_buffer().set_text("levantando o que enviar…")
        dentro = Gtk.ScrolledWindow(min_content_height=180, max_content_height=260,
                                    propagate_natural_height=True)
        dentro.set_child(self.relato_diagnostico)
        molde = Gtk.Frame()
        molde.add_css_class("view")
        molde.set_child(dentro)

        expansor = Adw.ExpanderRow(
            title="O que será enviado",
            subtitle="Já sem o seu nome, CPF, e-mail e caminhos pessoais")
        linha_previa = Adw.ActionRow(activatable=False)
        linha_previa.set_child(molde)
        expansor.add_row(linha_previa)
        grupo = Adw.PreferencesGroup()
        grupo.add(expansor)
        caixa.append(grupo)

        dialogo = Adw.MessageDialog(
            transient_for=self,
            heading="Relatar um problema",
            body="Conte o que aconteceu, com as suas palavras. O que você "
                 "escrever vai junto com um resumo do estado deste computador.")
        dialogo.set_extra_child(caixa)
        dialogo.add_response("fechar", "Cancelar")
        dialogo.add_response("enviar", "Preparando…")
        dialogo.set_response_enabled("enviar", False)
        dialogo.set_response_appearance("enviar", Adw.ResponseAppearance.SUGGESTED)
        dialogo.connect("response", self._respondeu_relato)
        self.relato_dialogo = dialogo
        dialogo.present()

        threading.Thread(target=self._preparar_relato, daemon=True).start()

    def _preparar_relato(self):
        """Fora da thread da interface: o diagnóstico e a prova de trabalho."""
        try:
            texto = diagnostico.coletar()
        except Exception as erro:  # noqa: BLE001
            registro.falha("não consegui coletar o diagnóstico", erro)
            texto = "(não consegui levantar o diagnóstico: %s)" % erro
        GLib.idle_add(self._mostrar_diagnostico, texto)

        try:
            desafio = relator.pedir_desafio()
        except Exception as erro:  # noqa: BLE001
            registro.falha("não consegui pedir o desafio ao servidor", erro)
            GLib.idle_add(self._relato_sem_servidor)
            return

        nonce = relator.resolver(desafio, parar=lambda: self.relato_cancelado)
        if nonce is None:
            return
        GLib.idle_add(self._prova_pronta, desafio, nonce)

    def _mostrar_diagnostico(self, texto):
        self.relato_diagnostico.get_buffer().set_text(texto)
        return False

    def _prova_pronta(self, desafio, nonce):
        self.relato_desafio = desafio
        self.relato_nonce = nonce
        if self.relato_dialogo is not None:
            self.relato_dialogo.set_response_label("enviar", "Enviar")
            self.relato_dialogo.set_response_enabled("enviar", True)
        return False

    def _relato_sem_servidor(self):
        if self.relato_dialogo is not None:
            self.relato_dialogo.set_response_label("enviar", "Servidor fora do ar")
        return False

    def _respondeu_relato(self, dialogo, resposta):
        self.relato_dialogo = None
        if resposta != "enviar":
            # Interrompe a prova de trabalho: sem isso, fechar o diálogo
            # deixaria um núcleo ocupado até ela terminar.
            self.relato_cancelado = True
            return

        buffer_mensagem = self.relato_texto.get_buffer()
        mensagem = buffer_mensagem.get_text(
            buffer_mensagem.get_start_iter(), buffer_mensagem.get_end_iter(), False)
        buffer_diagnostico = self.relato_diagnostico.get_buffer()
        texto = buffer_diagnostico.get_text(
            buffer_diagnostico.get_start_iter(),
            buffer_diagnostico.get_end_iter(), False)

        # O título sai da primeira linha do que a pessoa escreveu: pedir um
        # título à parte é pedir que ela resuma antes de contar.
        titulo = (mensagem.strip().splitlines() or ["Relato sem descrição"])[0]

        threading.Thread(
            target=self._enviar_relato,
            args=(titulo, mensagem, sanitizar.sanitizar(texto)),
            daemon=True).start()
        self.toasts.add_toast(Adw.Toast(title="Enviando o relato…"))

    def _enviar_relato(self, titulo, mensagem, texto):
        situacao, detalhe = relator.enviar(
            self.relato_desafio, self.relato_nonce,
            sanitizar.sanitizar(titulo), sanitizar.sanitizar(mensagem), texto,
            diagnostico.versao())
        GLib.idle_add(self._relato_enviado, situacao, detalhe)

    def _relato_enviado(self, situacao, detalhe):
        if situacao == "publicado":
            self.toasts.add_toast(Adw.Toast(title="Relato enviado. Obrigado."))
        elif situacao == "guardado":
            self.toasts.add_toast(Adw.Toast(
                title="Relato recebido. Será registrado assim que der."))
        else:
            self._avisar("Não consegui enviar o relato", detalhe or
                         "o servidor não aceitou o envio.")
        return False

    # ------------------------------------------------------------------
    def _oferecer_permissao(self, componente):
        """A permissão OPCIONAL de um componente, depois de ele ser instalado.

        Opcional de verdade: o aplicativo funciona sem ela, e o diálogo diz
        para que serve antes de pedir qualquer coisa. Só aparece se ela ainda
        não existe, e nunca antes de o componente estar instalado, porque até
        aí não havia motivo nenhum para pedi-la.
        """
        argumento, para_que = componente.permissao
        self._dialogo_de_comandos(
            "Quer poder %s?" % para_que,
            ["O %s funciona sem isso: assinando pelo PJe, quem entrega o "
             "documento é o navegador." % componente.nome,
             "Se você também for assinar arquivos guardados no computador, "
             "cole no terminal:"],
            [permissoes.comando_opcional(argumento)],
            confirmar="Copiar comando")

    def _clicou(self, botao, componente):
        partes = self.linhas[componente.chave]
        acao = decidir(partes["posto"], instalador.instalado(componente))

        if acao == "sincronizar":
            self.atualizar_componentes()
            self.atualizar_publicacao()
            self.atualizar_tokens()
            self.toasts.add_toast(Adw.Toast(
                title="O %s mudou por fora desta janela. Confira e clique de "
                      "novo." % componente.nome))
            return

        if acao == "remover":
            instalador.desinstalar(componente)
            self.atualizar_componentes()
            self._republicar()
            self.atualizar_publicacao()
            self.atualizar_tokens()
            self.toasts.add_toast(Adw.Toast(title="%s removido" % componente.nome))
            return

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
        self.atualizar_serie()
        self.toasts.add_toast(Adw.Toast(title="%s instalado" % componente.nome))
        if componente.permissao and not permissoes.tem_documentos():
            self._oferecer_permissao(componente)

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
