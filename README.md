# Âncora

Faz o certificado digital ICP-Brasil, em token A3 ou na nuvem, funcionar no
Linux: nos navegadores, no leitor de PDF e nos assinadores que você já usa,
inclusive quando eles são Flatpak. É para quem precisa assinar ou entrar com
certificado (PJe, e-SAJ, Projudi, eproc, Receita) e não quer instalar pacote de
fabricante no sistema.

O Âncora é o sucessor do
[flatpak-adv-br](https://github.com/LLawli/flatpak-adv-br), reescrito do zero.

## Como funciona

O aplicativo só publica. Drivers de token, assinadores e aplicativos como o
PJeOffice são **extensões**, que você instala pela própria janela. Cada extensão
só aparece na interface depois de instalada, e cada uma é baixada do site do
fabricante, na sua máquina, com a soma de verificação conferida.

O PJeOffice Pro do [FlatPark](https://flatpark.org) enxerga os tokens do Âncora
sem nenhuma configuração.

## Instalar

Ainda não há versão publicada. Para construir e instalar a partir do código:

```bash
flatpak install flathub org.gnome.Sdk//51 org.freedesktop.Sdk.Extension.rust-stable//26.08
flatpak run org.flatpak.Builder --user --install --force-clean build-dir flatpak/dev.lukakuuhaku.Ancora.yml
flatpak run dev.lukakuuhaku.Ancora
```

## Para quem vai mexer no código

[docs/arquitetura.md](docs/arquitetura.md) explica as peças, e
[docs/desenvolvimento.md](docs/desenvolvimento.md) explica como construir e
testar. `scripts/ci.sh` roda todos os gates, os mesmos da CI.

## Créditos

Saber qual versão de qual driver funciona com qual token, e que remendo faz
cada um rodar numa distribuição atual, é trabalho do Pedro HQB, no
[distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br) e no
[Tessera](https://github.com/pedrohqb/tessera).

## Licença

GPL-3.0-only. Veja [LICENSE](LICENSE).
