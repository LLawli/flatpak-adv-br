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
