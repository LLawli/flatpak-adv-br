# A versão com interface

Esta branch (`ui`) constrói um segundo produto a partir do mesmo repositório:
`dev.lukakuuhaku.AdvBr`, um aplicativo com janela, feito para ser publicado no
Flathub e usado sem terminal.

Ela existe separada porque a versão de linha de comando precisa ser testada
pelos testadores, com os tokens deles, antes de qualquer troca. Até lá, o que
está publicado não muda.

## A diferença que importa não é a janela

É o modelo de instalação. No pacote de linha de comando, cada driver é uma
**extensão Flatpak** construída na máquina de quem instala. Isso exige
`flatpak-builder` no host e, para uma interface fazer o mesmo, exigiria a
permissão `org.freedesktop.Flatpak`, que é a que o Flathub marca como insegura.
Seria pedir para ser recusado.

Aqui os componentes são **dados do aplicativo**: ele mesmo baixa o pacote do
fabricante e o extrai para
`~/.var/app/dev.lukakuuhaku.AdvBr/data/componentes/<chave>/`.

Isso foi medido antes de ser escolhido, e são dois fatos:

- um `.so` colocado nos dados do aplicativo **carrega** dentro do sandbox;
- um binário colocado lá **executa**.

O que se ganha: nada de flatpak-builder, nada de permissão perigosa, instalar
vira um clique e desinstalar vira apagar um diretório. O que se paga: as
bibliotecas de apoio que as extensões compilavam precisam vir no aplicativo (o
`gdbm` com a interface antiga, que o SafeSign exige), e não há
`flatpak update` para os componentes: quem atualiza é o catálogo do aplicativo.

## As peças

| arquivo | o que faz |
|---|---|
| `ui/catalogo.py` | o que dá para instalar: URL do fabricante, sha256 e o que extrair. Um componente novo é uma entrada aqui e nada mais. |
| `ui/deb.py` | lê um `.deb` sem o `ar`, que o runtime não traz. Formato `ar` em Python puro, e `zstd` pelo binário do runtime. |
| `ui/instalador.py` | baixa, confere o sha256, extrai para um diretório ao lado e só então troca. |
| `ui/pkcs11.py` | registra os módulos no p11-kit do sandbox e lê os tokens presentes, sem pedir PIN. |
| `ui/janela.py` | a tela. |

## Por que o `.deb` é lido em Python

O runtime não tem `ar`. A alternativa seria chamá-lo no host por
`flatpak-spawn`, o que traria de volta exatamente a permissão que este desenho
existe para não pedir. O formato `ar` são 60 bytes de cabeçalho por membro, e o
leitor inteiro cabe em meia tela.

O `data.tar` do SafeSign vem em **zstd**, que o `tarfile` só passou a ler no
Python 3.14 (o runtime traz o 3.13). O binário `zstd` está no runtime e é ele
que descomprime.

## A ordem da tela

De cima para baixo, na ordem em que a pessoa faz as perguntas:

1. **Seu certificado**: o que a máquina está enxergando agora. É o que ela veio
   saber.
2. **Navegadores e aplicativos**: levar isso para o Firefox, o Chrome, o Brave
   e o Papers que ela já usa. É um botão só, e é onde aparece o aviso de
   permissão quando falta acesso à home.
3. **Drivers de token**: OpenSC (já vem), SafeSign, SafeNet, SerproID.
4. **Assinadores**: Lacuna Web PKI, Certisign WebSigner, Certisign Desktop.
5. **Aplicativos**: PJeOffice Pro, que vem à parte por ser grande e por poucos
   precisarem dele.

Quem abre isto está com o token na mão e quer assinar, não configurar.

## O catálogo, e as duas formas de instalar

Sete componentes, todos baixados pela janela: três drivers proprietários, três
assinadores e o PJeOffice. Um clique instala, outro remove, e o terminal não
entra em momento nenhum.

Nada é redistribuído por este repositório. Cada pacote é baixado da origem
oficial na máquina de quem usa, com sha256 conferido antes de qualquer coisa
ser extraída, e essa é a razão de o modelo ser este: as licenças dos drivers
proprietários permitem ao licenciado usar e guardar cópia, não redistribuir, e
o PJeOffice é distribuído gratuitamente pelo CNJ sem licença publicada que
autorize terceiros a distribuí-lo. Publicar um repositório próprio não mudaria
isso: quem dita a regra é a licença de cada binário, não onde ele estaria
hospedado.

### O PJeOffice precisa de uma máquina virtual, e ela também é baixada

O .deb do CNJ não traz JVM nenhuma. A primeira tentativa aqui foi empacotá-lo
como extensão Flatpak, com o JRE do SDK dentro: funcionava, mas exigia
`flatpak-builder`, o SDK do GNOME e o de Java (mais de 1 GB para construir), e
um comando colado num terminal. Para um aplicativo cujo propósito é tirar o
terminal do caminho, isso é o problema, não a solução.

O que resolveu foi tratar a JVM como mais uma fonte do componente: o assinador
vem do pacote do CNJ, a máquina virtual vem da Azul (Zulu 11, GPLv2 com a
exceção de classpath), e o componente soma 155 MB baixados e 382 MB em disco.
Só para quem instala.

A JVM é a versão **com JavaFX**, que é maior, e a razão é o cortador de vídeo
de audiência: ele é JavaFX, e num JRE sem os módulos `javafx.*` a função
simplesmente não abre. São 90 MB baixados em vez de 42, e 260 MB em disco em
vez de 126. É também a mesma linha de JVM que o lançador oficial do CNJ usa
(`zulu-11-amd64`), o que faz deste o ambiente em que o programa é testado por
quem o escreve.

Que a cadeia de vídeo funciona dentro do sandbox foi medido, e não deduzido: o
`ffmpeg` que vem no pacote do CNJ é um ELF estático e roda; o toolkit JavaFX
inicia; e `javafx.media` abre um mp4 gerado por esse mesmo ffmpeg, informando
duração e dimensões corretas.

Uma consequência da escolha fica registrada aqui: **o atualizador automático do
CNJ é desligado na instalação.** Ele baixaria uma versão nova por cima desta,
sem conferir nada, dentro dos dados do aplicativo. Quem atualiza o que está
aqui é o catálogo, com sha256. A instalação falha alto se a linha `update.url=`
sumir do pacote: silêncio ali significaria o programa voltando a se atualizar
sozinho sem ninguém saber.

### Permissão nenhuma entra por padrão

O PJeOffice tem um assinador de arquivos avulsos que precisa ler documentos do
disco. Isso NÃO é o caminho normal de uso: assinando pelo PJe, quem entrega o
documento é o navegador, pelo servidor local em 127.0.0.1:8800, e nada é lido
da pasta de ninguém.

Por isso o manifesto não ganhou acesso a arquivos por causa dele. Depois de
instalar, e só se a permissão ainda não existir, a janela oferece:

    flatpak override --user --filesystem=xdg-documents dev.lukakuuhaku.AdvBr

Só a pasta Documentos, e só se a pessoa quiser. Permissão que o aplicativo não
usa é permissão que ele não deve ter, e uma que ele usa uma vez por ano não
justifica estar ligada o ano inteiro.

O mesmo vale ao contrário: este aplicativo não pede
`--talk-name=org.freedesktop.Flatpak`, que seria poder rodar qualquer comando
fora da caixa. É o que permitiria instalar coisas sozinho, e é caro demais
para o que compra.

### O que ainda pede terminal, e o que não pede mais

Instalar qualquer componente, inclusive o PJeOffice, é um clique. O que sobra
são as permissões de OUTROS programas, que este aplicativo não pode conceder:

- `flatpak override` num navegador em Flatpak: dá para fazer sem terminal, pelo
  Flatseal ou pela aba de permissões do gerenciador de aplicativos. O diálogo
  diz isso.
- `systemctl --user enable --now p11-kit-server.socket`: não tem equivalente
  gráfico. É um serviço do sistema de quem usa, e o diálogo diz que essa linha
  é a única que exige terminal, em vez de deixar a pessoa procurar num lugar
  onde ela não está.

## A série do p11-kit, e por que ela muda com a distribuição binária

A ponte PKCS#11 é o remoting do p11-kit: o p11-kit do sistema executa um
processo deste aplicativo e conversa com ele por um pipe. O que trafega ali é a
tabela de funções PKCS#11 serializada, e as duas pontas precisam concordar
sobre o formato dela. Quando não concordam, **nada recusa a conexão**: os slots
enumeram, o PIN é aceito, a lista de certificados aparece inteira, e toda
assinatura falha com `CKR_DEVICE_ERROR`. Como autenticar por certificado também
assina, no `CertificateVerify` do handshake TLS, o login no Projudi e no eproc
para de funcionar com o certificado aparecendo normalmente na lista.

O runtime traz a série 0.26. Debian trixie e Ubuntu 24.04 trazem a 0.25.

Na versão de linha de comando isso é resolvido antes de construir: o
`instalar.sh` compara as duas séries e embute um p11-kit da série do host. Esta
branch nasceu sem esse mecanismo, e o modelo de distribuição por repositório o
tornaria impossível de qualquer forma, porque some o momento do build na
máquina de quem instala.

Aqui a série virou um componente, como os drivers:

- **Descobrir a série do sistema de dentro do sandbox** parecia impossível e não
  é. O campo `library-version` do `p11-kit list-modules` é reportado pelo
  **módulo carregado**, não pela biblioteca do processo que pergunta. Então
  `ui/adv-br-serie` carrega o módulo de confiança **do host**, que o
  `--filesystem=host-os:ro` monta em `/run/host`, e lê a versão que ele reporta.
  Medido nesta máquina: `0.26`, igual ao que o host responde por fora.
- **Não serve o soname.** Aqui o host tem `libp11-kit.so.0.4.10` e o runtime
  `0.4.11`: sonames diferentes, mesma série, compatíveis. E as séries 0.23 e
  0.24 têm o mesmo soname `0.3.0`. O soname distingue por acaso.
- **Só a série necessária é instalada**, e só quando a divergência é detectada.
  Carregar três versões da mesma biblioteca seria carregar três superfícies de
  ataque para usar uma.
- **A ponte chama `p11-kit-remote` direto**, e não `p11-kit remote`: o
  `p11-kit` procura esse auxiliar por um caminho absoluto gravado em tempo de
  compilação, que não existe quando o componente vive nos dados do aplicativo.
- **Só o processo da ponte usa a biblioteca do componente.** O `LD_LIBRARY_PATH`
  é exportado dentro do `ui/adv-br-pkcs11`, e não no preparo comum: assinadores
  e PJeOffice continuam com a do runtime, porque não atravessam pipe nenhum.

Os artefatos são gerados por `bin/compilar-p11kit <série>`, do tarball oficial,
dentro do mesmo SDK do aplicativo, e publicados junto do repositório Flatpak. O
p11-kit é BSD-3-Clause, então redistribuí-lo é permitido, ao contrário de tudo
o mais neste catálogo.

## O log de cada módulo

Até aqui o aplicativo não escrevia uma linha de log. Pior: os três processos que
rodam fora da janela escrevem em stderr e ninguém guarda isso, porque quem os
inicia é o navegador ou o p11-kit do sistema. Quando um testador dizia "não
funcionou", não havia o que pedir a ele.

O caminho agora é um só, e vem de fora do código: `ui/registro.sh` aponta o
stderr do processo para `$XDG_DATA_HOME/logs/<módulo>.log`, e é incluído como
primeira coisa por cada lançador. Nenhum processo precisa saber escrever log, e
o **stdout continua intocado** — que é a única regra que não podia ser
quebrada, porque nele trafega o RPC do p11-kit e o native messaging.

Um arquivo por módulo: `janela`, `pkcs11`, `assinador-<chave>` e `app-<chave>`.
Um Lacuna que não responde e um Certisign que não responde são problemas
diferentes, e misturá-los num arquivo só custa justamente o tempo de separá-los
de novo. Rotação por tamanho, com um arquivo velho: a ponte roda a cada abertura
de navegador, e sem isso o log vira um arquivo grande demais para acompanhar um
relato de erro.

Do lado Python, `ui/registro.py` não abre arquivo nenhum: escreve no mesmo
stderr, com hora e origem. O que ele acrescenta é captura do que se perdia:
exceção não tratada, exceção dentro de callback do GTK (que não passa pelo
`sys.excepthook`, chega como "não levantável"), e os erros que o código engole
de propósito para não interromper o trabalho. Esses últimos são os que mais
importam: um `/proc/self/mountinfo` ilegível fazia a janela pedir permissões
que a pessoa já tinha, e um banco NSS que não abre é indistinguível de um banco
sem módulo registrado.

Em `ui/pkcs11.py`, os sete pontos em que `tokens()` devolvia lista vazia agora
dizem qual deles foi. "Nenhum certificado encontrado" é o relato mais comum que
se recebe, e separar "não há token espetado" de "a pilha PKCS#11 quebrou" é
metade do diagnóstico.

## O que ainda não existe

- desinstalar componentes que deixaram de existir no catálogo;
- atalho do PJeOffice no menu do sistema (hoje ele abre pela janela);
- VIDaaS, que é certificado em nuvem e não passa por PKCS#11 local.

## Armadilhas medidas aqui

- **O `gdk-pixbuf` não reconhece um SVG que comece com comentário.** Ele
  detecta o formato pelos primeiros bytes, e um comentário antes da tag `<svg>`
  empurra a assinatura para fora dessa janela. O `rsvg` lê, o `appstreamcli`
  não, e o build falha com `file-read-error` apontando para um arquivo que
  parece perfeito. O comentário vai depois da tag de abertura.
- **O OpenSC que vem no aplicativo precisa ser registrado como os outros.** O
  p11-kit lê `/etc/pkcs11/modules` e `/usr/share/p11-kit/modules`, e nenhum dos
  dois é `/app`: sem registrar, ele não existe para quem pergunta. O sintoma é
  a lista de tokens vazia mesmo antes de qualquer driver ser instalado.
- **O OpenSC instala completions em `/usr` no runtime GNOME.** No freedesktop
  não acontecia. `completiondir` resolve.
- **A `libserproidp11.so` não declara a `libgcc_s` de que depende.** Quem a abre
  com `dlopen` falha com `undefined symbol: _Unwind_Resume_or_Rethrow`, a menos
  que outra coisa já a tenha trazido para o processo, e aí o driver funciona por
  acidente. No modelo de extensões dava para corrigir no build, com `patchelf`;
  aqui o driver chega pronto pela rede, então quem carrega precisa fazer
  `LD_PRELOAD=libgcc_s.so.1` antes. Na janela isso exige reexecutar o
  interpretador: quando o Python já subiu, é tarde.
- **O aplicativo do SerproID reclama de FXML e segue funcionando.** Ele mostra
  `NullPointerException` em `ControllerInicio.exibirAtividade` porque o pacote
  do Serpro traz FXML de JavaFX 11 lido por um runtime JavaFX 8. Acontece igual
  fora do Flatpak, e não é este empacotamento.
- **A `libeToken` do SafeNet lê `/etc/eToken.conf` por caminho absoluto.** Não
  há variável que redirecione. No sandbox `/etc` é tmpfs e é gravável, então dá
  para copiar o arquivo para lá a cada execução; sem isso a biblioteca carrega,
  responde `C_GetFunctionList` e não encontra token nenhum, que é o sintoma mais
  caro de diagnosticar.
- **`ldd` avisa sobre permissão de execução em `.so` que veio de `.deb`.** O
  pacote traz as bibliotecas em 644 e nada as torna 755 na extração. É só o
  aviso do `ldd`: `dlopen` não pede bit de execução.
- **Uma extensão não vê o que o aplicativo base ainda não instalou.** O
  lançador do PJeOffice inclui `preparar-drivers.sh` e carrega o shim, os dois
  do aplicativo base. Se qualquer um mudar de lugar, a extensão continua
  construindo sem reclamar e falha só quando alguém a abre, semanas depois. Por
  isso o manifesto dela termina com `test -f` nos dois caminhos: o build quebra
  na hora certa.
- **O PJeOffice escuta em `*:8800`, não em `127.0.0.1:8800`.** É o comportamento
  dele, não do sandbox, e vale saber ao diagnosticar: com `--share=network` a
  pilha de rede é a do host, então o servidor fica visível para qualquer
  navegador da máquina, dentro ou fora de sandbox, e também para a rede local.
- **Lista de navegadores é lista de gente atendida.** A primeira versão daqui
  publicava para Firefox, Chrome, Chromium, Brave, Vivaldi, Edge e Opera, por
  uma tabela de caminhos. Quem usa LibreWolf, Zen, Floorp, Waterfox, Mullvad ou
  qualquer fork que apareça depois recebia o pior desfecho possível: publicar
  dizia que deu certo e o navegador continuava sem ver o certificado. Agora os
  navegadores são descobertos pelos marcadores que eles mesmos criam,
  `profiles.ini` para a família Firefox e `Local State` mais o diretório
  `Default` para a família Chromium. Exigir os dois marcadores do Chromium é o
  que separa navegador de aplicativo Electron, que também tem `Local State`.
- **Botão que decide pelo disco e rotula pela memória faz o oposto do que
  diz.** Aconteceu duas vezes, com o botão dos componentes e com o de publicar:
  a janela fica aberta enquanto algo muda por fora, e o clique executa o
  contrário do rótulo. No de publicar era pior, porque o resultado se parecia
  com nada ter acontecido: "Publicar" despublicava, e o aviso das permissões
  não aparecia porque publicação nenhuma tinha ocorrido. Ver `decidir()`.
- **O aplicativo se enxergava como um navegador.** Ele tem `--filesystem` para
  os diretórios de configuração dos navegadores, e o Flatpak monta cada um
  deles DUAS vezes: no caminho do host e dentro do config do próprio
  aplicativo. A descoberta então achava o Brave do host outra vez, agora dentro
  de `~/.var/app/dev.lukakuuhaku.AdvBr/config/`, e o tratava como navegador em
  sandbox. Como é o mesmo arquivo montado, a segunda passagem sobrescrevia o
  manifesto da primeira, e o Brave do host passava a apontar para um atalho que
  chama `flatpak-spawn`, que só existe DENTRO de um sandbox. O sintoma é a
  extensão dizendo que o assinador não está instalado, num navegador que estava
  publicado corretamente segundos antes. A casa do próprio aplicativo fica de
  fora da varredura.
- **Exceção dentro de um handler do GTK é silêncio.** O método que tratava a
  resposta de um diálogo saiu junto com um diálogo removido, e outros dois
  ainda o usavam. O `py_compile` passa, o import passa, a janela abre, e ao
  clicar o diálogo é construído, a exceção estoura antes do `present()`, e o
  PyGObject a escreve num stderr que ninguém lê. O botão parece não fazer nada,
  que é o sintoma mais caro de todos: não há erro para procurar. `tests/
  prova-atributos.py` procura `self._alguma_coisa` que não existe na classe, e
  foi verificado que ele pega exatamente esse caso.
- **O corpo de um `Adw.MessageDialog` não serve para comando de terminal.** É
  um parágrafo só, estreito e centralizado: uma linha como `flatpak override
  --user --filesystem=xdg-run/p11-kit/pkcs11 org.mozilla.firefox` não cabe na
  largura, e o diálogo cresce para baixo quebrando a linha em qualquer ponto
  até sair da tela. O que sobra é um bloco alto e estreito de fragmentos, que
  ninguém confere antes de colar. Os comandos passaram a ir num `extra_child`:
  fonte monoespaçada, uma linha cada, selecionáveis, com rolagem horizontal
  própria, e a lista inteira rolando na vertical se for longa. Medido: 668x550
  px para o aviso dos navegadores, contra uma tela de 1080 de altura.
- **Da série 0.24 para trás o p11-kit não compila com um compilador atual.** Ele
  usa uma variável chamada `thread_local`, que o C23 tornou palavra reservada, e
  o padrão do compilador do SDK já é o C23. `-Dc_std=gnu11` resolve. Sem isso, a
  compilação falha com "expected identifier before '=' token", que não diz nada
  sobre o que houve.
- **Um componente instalado nos dados do aplicativo não pode depender do
  `--prefix` com que foi compilado.** O `p11-kit` procura o auxiliar
  `p11-kit-remote` num caminho absoluto gravado no binário; instalado em
  `~/.var/app/.../componentes/`, esse caminho não existe e o comando responde
  "'remote' is not a valid command", como se a versão estivesse errada. Chamar o
  auxiliar diretamente resolve e não depende de onde o componente foi parar.
- **Um handler de log do GLib não serve para capturar o GLib.** A primeira
  versão do registro instalava `GLib.log_set_writer_func` e o log encheu de
  linhas como `128 do sistema gráfico: 93847593327200`: nessa API o campo da
  mensagem é um ponteiro, não uma string, e o que entrava no arquivo era o
  endereço. Não era preciso gancho nenhum: o GLib já escreve no stderr, e o
  stderr já está no arquivo.
- **O nome do módulo do assinador vem de um argumento do navegador.** Ele vira
  nome de arquivo, então um argumento com barras escreveria fora do diretório
  de logs. Medido antes de corrigir: `../../fuga` virava um arquivo com esse
  caminho. Hoje só sobrevivem letras, números e hífen.
