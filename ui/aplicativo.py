#!/usr/bin/env python3
"""A janela, montada e apresentada. Quem prepara o ambiente é o adv-br-ui.

Este arquivo não é o comando: o comando é o adv-br-ui, um shell que carrega o
preparo dos drivers e só então chama isto. A ordem importa. Uma parte do
preparo (o LD_PRELOAD da libgcc_s, que o driver do SerproID exige) só tem
efeito antes de o processo abrir a primeira biblioteca, e quando o Python já
está de pé é tarde.
"""
import sys

sys.path.insert(0, "/app/share/adv-br-ui")

import gi  # noqa: E402

gi.require_version("Adw", "1")
from gi.repository import Adw  # noqa: E402

import janela  # noqa: E402
import registro  # noqa: E402


registro.instalar_captura()


class Aplicacao(Adw.Application):
    def __init__(self):
        super().__init__(application_id="dev.lukakuuhaku.AdvBr")

    def do_activate(self):
        ativa = self.get_active_window()
        if not ativa:
            ativa = janela.Janela(self)
        ativa.present()


if __name__ == "__main__":
    sys.exit(Aplicacao().run(sys.argv))
