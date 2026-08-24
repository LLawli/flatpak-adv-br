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

Quem abre isto está com o token na mão e quer assinar, não configurar.

## O catálogo, e o que fica de fora dele

Os seis componentes do `main` estão aqui: três drivers proprietários e três
assinadores. Nenhum é redistribuído, todos são baixados do fabricante na hora,
e o sha256 de cada um é conferido antes de qualquer coisa ser extraída.

O **PJeOffice** não entra. Ele não é uma biblioteca que este aplicativo carrega,
é um programa Java com servidor próprio, e já existe empacotado em separado no
`PjeOffice-flatpak`, que consome as extensões de driver da versão de linha de
comando. Trazê-lo para cá significaria embutir um runtime Java inteiro num
aplicativo que hoje ocupa poucos megabytes, para duplicar um pacote que já
funciona.

Isso deixa uma ponta solta conhecida: no modelo desta branch os drivers vivem
em `~/.var/app/dev.lukakuuhaku.AdvBr/data/componentes/`, e não como extensões
Flatpak montadas dentro de outro aplicativo. O `PjeOffice-flatpak` continua
consumindo as extensões do `main`, e quem usar os dois terá o driver baixado
duas vezes. Resolver isso é decidir se o PJeOffice fala com este aplicativo
pela mesma ponte que os navegadores usam, o que exigiria dele permissão para
iniciar um Flatpak a partir de dentro do sandbox.

## O que ainda não existe

- desinstalar componentes que deixaram de existir no catálogo;
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
