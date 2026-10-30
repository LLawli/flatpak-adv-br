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
  token, e fechar o serviço não deixa nada para trás. Um token esperando a
  pessoa digitar o PIN não é confundido com um driver travado: a assinatura em
  andamento não é interrompida.
- Firefox, Chromium, Chrome, Edge, Brave, Papers e Okular em Flatpak passam a
  enxergar os tokens com `ancora configurar <id>`, que dá a permissão e acerta o
  banco de certificados de cada um, sem token duplicado nem PIN pedido duas
  vezes. `ancora desfazer <id>` tira só o que o Âncora pôs, e
  `ancora aplicativos` mostra o que falta em cada um.
- Firefox e navegadores derivados do Chromium instalados fora do Flatpak
  podem receber os tokens: `ancora nativos publicar firefox|sistema`, e
  `despublicar` para tirar.
- Antes de dar um aplicativo ou navegador por pronto, o Âncora confere, dentro
  dele, que o token aparece exatamente uma vez, e escolhe a forma de registro
  que garante isso. Sem token espetado também: a conferência usa um token de
  verificação próprio, visível só por alguns segundos.
- `ancora assinadores publicar` leva os assinadores instalados (como o Lacuna
  Web PKI e o Softplan WebSigner) a todos os navegadores, nativos e Flatpak,
  com os tokens dos drivers do Âncora. Num navegador Flatpak o assinador roda
  dentro dele, sem dar ao navegador permissão de sair do sandbox.
  `ancora assinadores despublicar` tira tudo.
- A janela mostra os tokens conectados e os certificados de cada um (titular,
  CPF ou CNPJ, emissor e validade), com destaque para o que venceu ou vence nos
  próximos 30 dias. Ela lista as extensões instaladas, com um interruptor para
  cada driver e cada assinador e o motivo de um driver que não responde, e os
  aplicativos Flatpak e navegadores do sistema, cada um com o seu interruptor.
- Abrir a janela sobe o serviço, que continua depois de ela fechar. No menu,
  "Iniciar com a sessão" pede ao sistema que o suba no login (onde houver o
  portal Background, como no GNOME), e "Reiniciar o serviço" faz uma extensão
  instalada depois passar a valer, o que a linha dela também oferece.
- `ancora certificados <socket>` lista, em JSON, os certificados dos tokens de
  um driver, sem pedir PIN.
- Adicionar extensões pela janela: o botão `+` da seção Extensões busca, nos
  remotes que você já tem, os drivers, assinadores e aplicativos para o
  Âncora, e instala com um clique. Cada extensão instalada mostra quando há
  versão nova e pode ser removida pelo menu dela. Como o Flatpak só monta
  extensões na partida, um aviso no alto da janela oferece reiniciar o Âncora,
  o que leva cerca de um segundo.
- `ancora disponiveis`, `ancora instalar`, `ancora atualizar` e
  `ancora remover` fazem o mesmo pelo terminal.
- O cliente PC/SC vem no Âncora, para os drivers de token falarem com as
  leitoras do sistema.

### Corrigido

- Um driver sem nenhum token (uma leitora sem cartão, por exemplo) deixa de
  ser tratado como travado e parado: a lista vazia de slots fazia a sonda
  repetir o pedido para sempre.
- Os drivers que falam com o fabricante durante a assinatura (como o da
  Certisign) passam a ter rede.

### Mudado

- `ancora assinadores publicar` liga todos os assinadores instalados, e
  `despublicar` desliga todos: é a mesma escolha que a janela mostra.
