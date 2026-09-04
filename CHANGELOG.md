# Registro de mudanças

Todas as mudanças relevantes deste projeto. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e a numeração
segue o [SemVer](https://semver.org/lang/pt-BR/).

## [1.1.0] - 2026-09-04

### Adicionado

- **RemoteID: o certificado em nuvem da Certisign passa a funcionar no Linux.**
  A Certisign publica aplicativo só para Windows e macOS; numa máquina Linux o
  certificado simplesmente não existia. Agora ele aparece como token para o
  navegador, para o Papers, para o Lacuna Web PKI, para o Softplan WebSigner e
  para o PJeOffice, como qualquer outro. A chave privada continua no HSM da
  Certisign e nunca esteve na sua máquina.

  É o primeiro componente que **não** é baixado do site de um fabricante: o
  [RemoteID-linux](https://github.com/LLawli/RemoteID-linux) é GPLv3, então
  quem compila e publica é este projeto, com `bin/compilar-remoteid`, dentro do
  mesmo SDK do runtime. Sua máquina baixa 4 MB e confere o sha256.

  O que foi confirmado com conta real é a autorização por PIN e código do
  autenticador; a aprovação por celular está implementada e nunca passou por
  uma assinatura de verdade.

- **`adv-br-remoteid`**, para usar o RemoteID pela linha de comando de dentro do
  aplicativo, e para ligar o modo de teste contra um servidor RemoteID falso
  sem tocar na conta real:

  ```sh
  flatpak run --command=adv-br-remoteid dev.lukakuuhaku.AdvBr mock
  flatpak run --command=adv-br-remoteid dev.lukakuuhaku.AdvBr teste http://localhost:8799
  ```

  O interruptor é um arquivo, e não uma variável de ambiente, porque precisa
  valer também para a ponte que o p11-kit inicia sob demanda e para o assinador
  que o navegador executa — nenhum dos dois herda o ambiente de um terminal.

- **Atalho de menu do RemoteID, escrito pelo próprio projeto**, nos dois
  pacotes. O `.desktop` do componente é usado como está e só duas linhas mudam,
  porque só duas não fazem sentido fora da máquina de quem o escreveu: o `Exec`,
  que precisa entrar pelo `adv-br-aplicativo` para o preparo dos drivers
  acontecer também pelo menu, e o `Icon`. Nome, descrição traduzida,
  palavras-chave e `StartupWMClass` são metadados do autor, e uma cópia nossa
  divergiria — é a mesma regra dos manifestos de native messaging dos
  assinadores. Componente sem `.desktop` próprio continua com um composto aqui.
- **O ícone do componente vale no atalho**, nos dois pacotes. O `Icon=` passa a aceitar um ícone que o componente traga: um caminho
  absoluto dentro dos dados do aplicativo, porque o atalho vive no menu do host,
  que não tem tema de ícone nosso nem enxerga `/app`. Componente sem ícone
  próprio continua usando o do aplicativo.

- **O relato de erro passa a levar o diagnóstico do próprio RemoteID**, e a
  ponte PKCS#11 passa a dizer o que fez. Os dois primeiros relatos de quem foi
  usar o certificado em nuvem chegaram inconclusivos: o `app-remoteid.log`
  trazia avisos do GTK e nada mais, e o `pkcs11.log` trazia três marcadores de
  sessão vazios — a ponte subia e não dizia qual módulo carregava, com qual
  p11-kit, nem quanto tempo levava.

  O diagnóstico do RemoteID é uma **exceção deliberada e datada**: ele
  identifica o titular do certificado, e por isso mora fora do diretório que o
  relato varre. Entra por um caminho próprio, das três execuções mais recentes,
  com teto de 20 KB, e sai daqui quando o RemoteID deixar a fase de teste. O
  que ele grava já vem redigido pelo próprio: senha, PIN e OTP nunca entram, e
  token aparece só como impressão digital. O CPF, a nossa sanitização come. E
  a pessoa continua vendo o texto inteiro antes de enviar.

- **O aplicativo passa a dizer quando uma leitora travada está escondendo os
  certificados dos outros programas.** Ele já contornava a situação para si; o
  problema é que contornar em silêncio deixa a pessoa com o certificado
  aparecendo aqui e sumido no PJeOffice, no navegador e no Papers — e a queixa
  que chega é "o certificado não aparece", nunca "a minha leitora está
  ocupada". Agora o relato traz uma linha `ATENÇÃO:` com a causa mais comum (o
  gnupg segurando o cartão) e os dois comandos que resolvem, e o
  `./diagnostico.sh` ganhou uma seção que detecta o caso.

- **RemoteID v0.1.2**, que faz o PJeOffice conseguir assinar. O módulo
  implementava só o `C_Sign` de um tiro; quem assina em fluxo — o BouncyCastle,
  que é como o PJeOffice assina — recebia `CKR_FUNCTION_NOT_SUPPORTED` no
  `C_SignUpdate`. Medido antes e depois, na mesma JVM que o PJeOffice usa:
  `initSign` → `C_SignUpdate` → `C_SignFinal`, 256 bytes de assinatura.

### Corrigido

- **O relato levava o diagnóstico do RemoteID errado.** Na primeira vez em que
  a seção foi exercitada de verdade, ela veio com o diretório de produção,
  cheio de `sessao.inicio` contra a Certisign, enquanto tudo o que interessava
  tinha acontecido em modo de teste, contra o mock, no outro diretório. Uma
  seção que chega com o log errado é pior que nenhuma: responde a pergunta com
  confiança e responde errado. Agora o modo decide qual é lido, e o relato diz
  de qual dos dois veio.
- **A conta do modo de teste podia morar num lugar que some no logout.** O
  `/tmp` do sandbox é compartilhado entre as instâncias do aplicativo — ao
  contrário do que este projeto assumia —, mas mora no diretório de execução da
  sessão. Quando o RemoteID criava `/tmp/remoteid-teste` antes do preparo, o
  estado ficava lá e seria perdido no logout. O preparo agora resgata o que
  encontrar para os dados do aplicativo, e só avisa (sem escolher) quando há
  conta gravada dos dois lados.

- **Um slot de leitora com defeito escondia todos os certificados da janela**,
  e não só o dele. `C_GetSlotList(CKF_TOKEN_PRESENT)` reprova a chamada inteira
  quando um único slot recusa responder, então uma YubiKey em modo
  OTP+FIDO+CCID com o `scdaemon` do gnupg segurando a interface fazia sumir
  até o certificado em nuvem, que não usa leitora nenhuma. Agora, quando essa
  pergunta falha, o aplicativo pergunta por todos os slots e descarta um a um
  — a mesma tolerância que o `critical: no` dos `.module` já dava do outro
  lado. Vale para qualquer leitora com defeito, não só para essa.

- **Os assinadores em navegador não enxergavam token nenhum.** O
  `adv-br-assinador` preparava o ambiente mas nunca registrava os módulos no
  p11-kit do sandbox, e `/etc/pkcs11/modules` é um tmpfs recriado a cada
  execução — então o `/pkcs11/adv-br.so` que a extensão usa respondia por um
  proxy vazio. A extensão conectava, o `getVersion` respondia, e a lista de
  certificados voltava vazia, como se não houvesse token. Sumia tudo, não só o
  driver novo. Ver `docs/ARMADILHAS.md`.

### Notas para quem for mexer

- O socket entre o módulo PKCS#11 do RemoteID e o aplicativo dele **não** pode
  ficar no `$XDG_RUNTIME_DIR`: ele é privado de cada instância de `flatpak run`,
  e o módulo e o aplicativo nunca rodam na mesma. O sintoma seria o certificado
  aparecer e só a assinatura falhar. Ver `docs/ARMADILHAS.md`.
- O aplicativo do RemoteID registra um id próprio no barramento de sessão, e o
  filtro que o Flatpak monta por padrão só deixa possuir nomes que comecem pelo
  id do pacote. Sem `--own-name` ele morre no arranque com
  `Failed to register: ...ServiceUnknown`, uma mensagem que não fala em
  barramento, nome nem permissão. Ver `docs/ARMADILHAS.md`.
- O diagnóstico do RemoteID fica junto do estado, e **não** em
  `$XDG_DATA_HOME/logs`: o botão "Relatar um problema" varre esse diretório e
  envia o que encontra, e o diagnóstico dele identifica o titular do
  certificado. Ele já redige senha, PIN e OTP por conta própria; quem o envia é
  a pessoa, de propósito.

## [1.0.4] - 2026-08-25

### Corrigido

- **A escala do monitor deixa de depender de a janela ter sido aberta.** Na
  1.0.3 quem anotava o número era a janela do Certificado Digital, então abrir o
  PJeOffice direto pelo atalho, numa máquina onde a janela nunca tinha rodado,
  não corrigia nada. Agora o próprio lançador pergunta ao compositor, sem abrir
  janela nenhuma. Com mais de um monitor ele usa a maior escala: sem janela não
  há como saber em qual deles o assinador vai abrir, e escala de menos é a tela
  borrada que motivou tudo isto, enquanto escala de mais é uma janela grande e
  legível.

## [1.0.3] - 2026-08-25

Dois pedidos de quem está testando, e uma correção de algo que quebrei na 1.0.2.

### Adicionado

- **Atalho no menu para os componentes que trazem aplicativo**, hoje o PJeOffice
  e o SerproID. Abrir o assinador deixa de custar abrir o Certificado Digital,
  rolar até o fim da lista e clicar em abrir: ele é usado sozinho, várias vezes
  por dia, e não faz parte do fluxo de instalar componente. O atalho nasce ao
  instalar e some ao desinstalar.
- **A escala do monitor chega à máquina virtual Java do PJeOffice.** Ele é Swing
  rodando por XWayland, onde não descobre escala fracionária sozinho: num monitor
  a 125% ou 150% a janela sai borrada. A janela deste aplicativo, que é GTK e
  sabe a escala, anota o número, e o lançador o repassa à JVM.

### Corrigido

- **A barra de rolagem do diálogo de comandos escondia o comando.** A correção
  da 1.0.2 desligou a rolagem flutuante, e com isso a barra passou a ocupar
  altura numa caixa que não cresceu: em todo comando longo o suficiente para ter
  barra, o texto sumia. Agora a barra volta a flutuar, sobre um espaço reservado
  para ela, e o comando é um campo de texto de verdade, que rola sozinho
  enquanto se arrasta a seleção e aceita Ctrl+A e Ctrl+C.

## [1.0.2] - 2026-08-25

Duas correções vindas de quem está testando a versão publicada.

### Corrigido

- **Aplicativos com Chromium por dentro apareciam como navegador.** Steam,
  Discord e Spotify embutem o motor inteiro e criam os mesmos marcadores que um
  navegador deixa: o arquivo `Local State` e o diretório `Default`. O que eles
  não têm é gerenciador de perfis, e é isso que passa a separar um do outro.
  Publicar para algo que não é navegador escreve manifesto onde ninguém lê, e
  falha em silêncio.
- **O mesmo navegador era descoberto duas vezes.** A varredura olha a casa e o
  `.config` separadamente, e como a primeira também desce um nível, o mesmo
  caminho saía duas vezes. O sintoma visível era o diagnóstico dizendo
  "chromium, chromium, chromium"; o invisível era publicar duas vezes para o
  mesmo navegador.
- **A barra de rolagem cobria o comando que se quer copiar.** No GTK4 ela flutua
  sobre o conteúdo, e num comando de uma linha só ficava exatamente em cima do
  texto, impedindo selecionar com o mouse. Agora ela tem espaço próprio.

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
