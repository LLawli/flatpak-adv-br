"""Registro do lado Python: tudo que a janela sabe e ninguém veria.

O caminho é um só e vem de fora: o lançador em shell (ui/registro.sh) já
apontou o stderr deste processo para o arquivo do módulo. Aqui não se abre
arquivo nenhum; o que se faz é escrever no stderr com hora e origem, e garantir
que o que hoje se perde passe por aqui.

O que se perde hoje, e que este módulo existe para capturar:

  - exceções não tratadas na janela. Dentro de um handler do GTK elas não
    derrubam nada nem aparecem: o PyGObject as escreve num stderr que ninguém
    lê. Foi assim que o aviso das permissões dos navegadores passou semanas sem
    aparecer, com o botão parecendo não fazer nada;
  - os avisos do próprio GLib e do GTK;
  - os erros que o código engole de propósito para não interromper o trabalho
    (um banco NSS ilegível, um manifesto de navegador quebrado), que são
    justamente os que explicam um "aqui não funcionou".
"""
import datetime
import sys
import traceback

MODULO = "janela"


def _agora():
    return datetime.datetime.now().strftime("%H:%M:%S")


def registrar(mensagem, *args):
    """Uma linha no log. Nunca levanta: registrar não pode quebrar o programa."""
    try:
        texto = mensagem % args if args else mensagem
        sys.stderr.write("%s %s: %s\n" % (_agora(), MODULO, texto))
        sys.stderr.flush()
    except Exception:  # noqa: BLE001
        pass


def falha(mensagem, erro):
    """Uma linha para o erro que o código decidiu não propagar.

    O tipo da exceção entra junto: "não consegui ler o banco" com
    PermissionError e com FileNotFoundError levam a conversas diferentes com
    quem relatou o problema.
    """
    registrar("%s: %s: %s", mensagem, type(erro).__name__, erro)


def instalar_captura():
    """Faz o que ninguém veria passar pelo log. Chamado uma vez, na abertura.

    Só as exceções. Os avisos do GLib e do GTK não precisam de gancho nenhum:
    eles já vão para o stderr, e o stderr deste processo já está no arquivo.
    A primeira versão daqui instalava um GLib.log_set_writer_func para
    "capturá-los", e o resultado foi um log cheio de linhas como
    "128 do sistema gráfico: 93847593327200": nessa API o campo da mensagem é
    um ponteiro, não uma string, e o que entrava no arquivo era o endereço.
    """
    def nas_excecoes(tipo, valor, pilha):
        registrar("exceção não tratada:\n%s",
                  "".join(traceback.format_exception(tipo, valor, pilha)).strip())
        sys.__excepthook__(tipo, valor, pilha)

    sys.excepthook = nas_excecoes

    # Exceção dentro de callback do GTK não passa pelo excepthook: ela chega
    # aqui, como "não levantável". É a que mais custou neste projeto, porque
    # não derruba nada e não aparece.
    anterior = sys.unraisablehook

    def nas_nao_levantaveis(evento):
        registrar("exceção sem quem a receba: %s: %s",
                  type(evento.exc_value).__name__, evento.exc_value)
        if evento.exc_traceback is not None:
            registrar("%s", "".join(traceback.format_tb(evento.exc_traceback)).strip())
        anterior(evento)

    sys.unraisablehook = nas_nao_levantaveis
