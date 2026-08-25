# Registro de mudanças

Todas as mudanças relevantes deste projeto. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e a numeração
segue o [SemVer](https://semver.org/lang/pt-BR/).

## [1.0.1] - 2026-08-25

### Corrigido

- **O aplicativo não dizia, e nem tornava possível, o caminho que a extensão do
  assinador precisa.** As extensões (Lacuna Web PKI, WebSigner) pedem um arquivo
  de driver na aba "Cripto Dispositivos", e as opções que elas oferecem prontas
  apontam para `/usr/lib` do sistema, onde não há nada deste pacote. Sem isso a
  pessoa instalava o assinador, instalava a extensão, e o site continuava
  dizendo que nenhum certificado foi encontrado.

  O caminho `/pkcs11/adv-br.so` passa a existir dentro do aplicativo, criado a
  cada execução de um lançador, e responde por todos os drivers instalados,
  inclusive os que forem instalados depois. Ele aponta para o shim, e não para o
  p11-kit-proxy do runtime, porque o PJeOffice grava o caminho já canonizado na
  configuração dele: com o proxy no fim do link, uma atualização de runtime
  tiraria o driver da pessoa sem dizer nada.

  A janela passa a ensinar o caminho em dois lugares: na descrição do grupo de
  assinadores, sempre visível, e num diálogo com botão de copiar assim que um
  assinador termina de instalar.

## [1.0.0] - 2026-08-25

O aplicativo com janela, `dev.lukakuuhaku.AdvBr`, e a distribuição própria.

A numeração passa a ser do repositório, e não de cada pacote: a 0.1.0 foi a
versão de linha de comando, e daqui em diante os dois produtos andam juntos no
mesmo número. É 1.0.0 porque o que estreia aqui é um produto que alguém instala
sem terminal, e isso é um compromisso com a interface, não só uma versão a mais.

### Adicionado

- Aplicativo com janela: instala um driver com um clique, publica o *native
  messaging* para os navegadores e diz o que fazer quando falta permissão, em
  vez de falhar calado.
- Catálogo de componentes baixados sob demanda: drivers SerproID e SafeNet,
  assinadores, e o PJeOffice. Cada um mostra o tamanho do download antes, e
  abre o aplicativo próprio quando existe.
- PJeOffice construído na máquina de quem usa, com a máquina virtual junto
  (Zulu 11 com JavaFX). Não é redistribuível, então o que viaja é a receita, e
  não o binário.
- Descoberta dos navegadores instalados, no lugar de uma lista fixa, incluindo
  os que estão em Flatpak, com as permissões que cada um exige.
- Compatibilidade de série do p11-kit resolvida por componente: o aplicativo
  descobre a série dos dois lados da ponte e oferece o ajuste só quando o
  sistema precisa dele. Sem isso o token aparece, o PIN é aceito, e toda
  assinatura falha.
- Log por módulo em `$XDG_DATA_HOME/logs/`, com o stdout intocado, que é por
  onde trafegam o RPC do p11-kit e o *native messaging*.
- Botão de relatar um problema, com prévia editável do que será enviado, e o
  serviço que recebe o relato e abre a issue.
- Repositório Flatpak próprio, assinado, com `bin/publicar` fazendo o caminho
  inteiro (construir, exportar, assinar, gerar os deltas e enviar).
- Cortador de vídeo de audiência.
- `docs/ui.md` e `docs/deploy.md`, com as armadilhas medidas e o roteiro do
  deploy na ordem em que ele acontece.
- O aplicativo com interface entrou no CI.

### Modificado

- **Instalar deixou de exigir `flatpak-builder`, o SDK do GNOME e mais de 1 GB
  de cache para um pacote de poucos megabytes.** Passa a ser adicionar o
  remoto e instalar. O `./instalar.sh` continua para quem quiser construir,
  mas deixou de ser a única porta.
- O PJeOffice deixou de ser extensão Flatpak e virou componente. O que o
  prendia era a máquina virtual; tratando ela como mais uma fonte do
  componente, instalar virou um clique.
- Os diálogos que entregam comandos ficaram legíveis, e mostram o comando em
  vez de pedir permissão para executá-lo.

### Corrigido

- Botão que fazia o oposto do que dizia.
- Diretório do SerproID criado onde a biblioteca de fato o procura.
- A configuração do PJeOffice sobrevive a fechar o aplicativo.
- A republicação para os navegadores acontece também depois de instalar, e não
  só depois de remover.
- O aplicativo saiu da própria lista de navegadores.
- Voltou o método que fazia o popup das permissões aparecer.

### Segurança

- Nada proprietário é redistribuído, nem pelo repositório próprio: quem dita é
  a licença de cada binário, não onde ele estaria hospedado.
- A chave privada de assinatura nunca sai da máquina de quem publica. O
  servidor guarda arquivos assinados e não sabe assinar nada.
- O token do GitHub fica no servidor, e não dentro do aplicativo: token
  embutido em programa distribuído não é segredo. O que o aplicativo manda é
  texto já sanitizado, com uma prova de trabalho para encarecer o abuso.
- Nenhum lançador escreve mais no próprio stdout, o que corrompia o protocolo
  em vez de falhar de forma visível.

## [0.1.0] - 2026-08-24

Primeira versão. O [sora-adv-br](https://github.com/LLawli/sora-adv-br) com
Flatpak no lugar do distrobox.

### Adicionado

- Pacote base `io.github.llawli.AdvBr`: OpenSC, a biblioteca cliente do PC/SC,
  o shim PKCS#11 e a ponte para o host. Poucos megabytes, porque tudo o que é
  opcional é extensão, instalada quando for usada:
  - drivers de token SafeSign, SafeNet e SerproID;
  - assinadores Lacuna Web PKI, Softplan WebSigner e Certisign WebSigner;
  - PJeOffice Pro, com atalho próprio no menu.

  Dá para instalar só um driver e um assinador, ou tudo, ou um hoje e outro
  amanhã: o `./instalar.sh` é idempotente e não refaz o que já está pronto.
- Ponte PKCS#11 para o host por `.module` com `remote: |flatpak run`, um
  processo por driver, o que isola o driver que derruba quem o carregou e evita
  devolver ao host as âncoras de confiança que ele já tem.
- Publicação de *native messaging* para navegadores do host e em Flatpak, com
  atalhos separados (`flatpak run` e `flatpak-spawn --host flatpak run`).
- Registro nos bancos NSS sem `nss-tools`: `host/nssdb.py` edita o
  `pkcs11.txt`. Bancos que vivem no home real recebem os dois módulos, o do
  host e o do sandbox, o que faz o mesmo arquivo servir ao Papers em Flatpak e
  aos programas do host.
- `/pkcs11/adv-br.so` dentro do sandbox: um caminho só, digitável na aba
  "Cripto Dispositivos" das extensões, que responde por todos os drivers.
- Compatibilidade automática de série do p11-kit: quando o host está numa série
  diferente da do runtime (Debian trixie e Ubuntu 24.04 trazem 0.25 contra a
  0.26 do runtime), o instalador compila um p11-kit da série do host, isolado,
  e só a ponte o usa. Sem isso, o token aparece, o PIN é aceito e toda
  assinatura falha.
- Instalação a partir de um repositório local quando o `--install` do
  flatpak-builder falha por causa do remoto. A construção já terminou, e perder
  meia hora de compilação por um Flathub lento no último passo é desnecessário.
- `./diagnostico.sh`, `./host/testar-pkcs11.sh`, `./host/testar-assinador.sh` e
  `./tests/testar.sh`.
- `docs/arquitetura.md` e `docs/ARMADILHAS.md`, com o que foi medido.
- CI estático e workflow de release por tag; `bin/release` para soltar uma.
