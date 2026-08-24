#!/bin/sh
# Executado pelo lançador antes da JVM subir. Ver drivers/README.md.
#
# A libserproidp11 faz readdir em ~/.config/serproid/certificados assim que é
# carregada. Se o diretório não existir, o ponteiro vem nulo e ela derruba com
# SIGSEGV o processo que a carregou. Quem carrega é o assinador, com todos os
# módulos configurados. Sem este mkdir, instalar o SerproID impede assinar com
# qualquer outro token.
set -eu

mkdir -p "$HOME/.config/serproid/certificados"
