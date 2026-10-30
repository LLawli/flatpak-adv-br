# Changelog

Todas as mudanças visíveis para quem usa ficam aqui. O formato segue o
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e o projeto usa
[versionamento semântico](https://semver.org/lang/pt-BR/).

## [Unreleased]

### Adicionado

- A janela do Âncora (`dev.lukakuuhaku.Ancora`), com nome, ícone e entrada no
  menu de aplicativos, empacotada como Flatpak sobre o GNOME 51.
- Extensões de driver, assinador e aplicativo
  (`dev.lukakuuhaku.Ancora.{Driver,Assinador,App}.<Nome>`), cada uma descrita
  por um `ancora.toml`. O contrato está em `docs/extensoes.md`.
- `flatpak run --command=ancora dev.lukakuuhaku.Ancora extensoes` lista as
  extensões instaladas e diz o motivo de cada uma que estiver quebrada.
- `ancora servir`: publica os drivers instalados no p11-kit do sistema, para
  os navegadores e aplicativos (inclusive Flatpak, como o PJeOffice Pro do
  FlatPark). Um driver que trava sai da publicação em vez de travar quem abre o
  token, e fechar o serviço não deixa nada para trás.
- Firefox, Chromium, Chrome, Edge, Brave, Papers e Okular em Flatpak passam a
  enxergar os tokens com `ancora configurar <id>`, que dá a permissão e acerta o
  banco de certificados de cada um, sem token duplicado nem PIN pedido duas
  vezes. `ancora desfazer <id>` tira só o que o Âncora pôs, e
  `ancora aplicativos` mostra o que falta em cada um.
