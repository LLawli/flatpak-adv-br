# Arquitetura

Para quem vai mexer no projeto. O que ele resolve, e para quem, está no
[README](../README.md).

## A divisão

O **aplicativo base** só publica: leva para os navegadores e aplicativos do
sistema o que está dentro do sandbox. Isso inclui a ponte PKCS#11 pelo p11-kit
do host, o registro nos bancos NSS e os manifestos de native messaging.

Todo o resto é **extensão Flatpak** do `dev.lukakuuhaku.Ancora`:

| família | ref | o que traz |
|---|---|---|
| drivers de token | `dev.lukakuuhaku.Ancora.Driver.<Nome>` | a biblioteca PKCS#11 do fabricante |
| assinadores | `dev.lukakuuhaku.Ancora.Assinador.<Nome>` | o programa que o navegador chama por native messaging |
| aplicativos | `dev.lukakuuhaku.Ancora.App.<Nome>` | um aplicativo com janela (PJeOffice, por exemplo) |

O aplicativo base não tem tabela de extensões. Cada extensão traz um
`ancora.toml` dizendo o que é, o que publica e que programas oferece, e a
interface de uma extensão só existe enquanto ela está instalada. O contrato
está em [extensoes.md](extensoes.md).

Não há ponto de extensão público para outros aplicativos montarem os drivers,
como o flatpak-adv-br tinha para o PjeOffice-flatpak: um app que queira os
tokens os recebe pelo p11-kit do host, sem depender do Âncora.

## A publicação dos drivers

```
token USB ──► pcscd (host) ──--socket=pcsc──┐
┌──────────────── sandbox do Âncora ─────────▼──────────────┐
│ ancora servir                                              │
│   um p11-kit server por módulo, cada um carregando o driver│
│   $XDG_RUNTIME_DIR/ancora/<driver>                         │
│   sonda a cada 10 s (ancora vigiar, um processo por driver)│
└────────────────────────────────────────────────────────────┘
        ▲ remote: unix:path=... em ~/.config/pkcs11/modules/ancora-<driver>.module
┌──────── host ──────────────────────────────────────────────┐
│ p11-kit-proxy (aplicativos nativos)                        │
│ p11-kit-server.socket do usuário (aplicativos Flatpak com  │
│   --filesystem=xdg-run/p11-kit/pkcs11, como o PJeOffice)   │
└────────────────────────────────────────────────────────────┘
```

`ancora servir` (`src/servico.rs`) sobe um `p11-kit server` por módulo
PKCS#11 das extensões de driver e o sonda a cada 10 s, com limite de 10 s.
A sonda é um processo por driver (`ancora vigiar`) que carrega o driver uma
vez e responde a cada pedido: carregar o driver é o que mexe no cartão. O
SafeSign liga o cartão e manda dezenas de comandos em cada `C_Initialize`, e o
desliga no `C_Finalize`; medido com um cartão que ele reconhece, sondar por
conexões novas custava 6 ciclos de energia e 474 comandos por minuto, e a
sonda que fica carregada, nenhum ciclo e 3 leituras por consulta. Com o GD
Burti, o primeiro jeito acompanhou o pcscd perder a comunicação USB com o
token. Ela não conecta pelo socket do servidor: o `p11-kit server` decide os
tokens de uma conexão quando ela abre (`p11_kit_remote_serve_tokens`), e uma
conexão mantida não veria um token conectado depois. Só o que respondeu da
última vez fica publicado
(`src/publicacao.rs`). Um driver que não responde sai da publicação e é
reiniciado, e depois de 3 vezes seguidas é parado (`src/supervisao.rs`): um
driver travado e publicado travaria todo aplicativo que abrisse o p11-kit.

Sem resposta não quer dizer travado. Se, quando a sonda passa do limite, o
servidor tem alguma conexão aberta (a sonda não é uma), o driver pode estar
só ocupado: uma
pessoa digitando o PIN num leitor com teclado segura a transação do cartão, e
um driver que fala com o cartão para listar tokens espera por ela. Nesse caso
o driver sai da publicação até voltar a responder, mas não é reiniciado nem
conta para parar: reiniciar mataria a conexão de quem está assinando
(medido: o login era derrubado em 11 s com `CKR_DEVICE_ERROR`; com esta regra,
termina em 25 s). `tests/e2e/supervisao.sh` confere.

A sonda lista os slots pela chamada crua (`sonda::listar_slots`), e não pelo
`get_slots_with_token` do cryptoki. Com zero slots (o driver de uma leitora sem
cartão), o cryptoki pede a lista de novo com um buffer de tamanho zero, o RPC do
p11-kit responde `CKR_BUFFER_TOO_SMALL`, e ele repete para sempre com a mesma
conta: medido em VM, o driver era parado depois de três sondas, com a carga em
16. `tests/e2e/semtoken.sh` confere com um módulo sem slot (`vazio.c`).

O aplicativo base traz o cliente PC/SC (`libpcsclite.so.1`, pcsc-lite 2.5.1),
que todo driver usa para falar com o `pcscd` do host pelo `--socket=pcsc`: o
runtime não o traz, e um só para todos os drivers evita versões diferentes.

O sandbox tem rede (`--share=network`): os drivers rodam no serviço do Âncora,
alguns falam com o fabricante durante a assinatura (o da Certisign, por
exemplo), e uma extensão não acrescenta permissões ao sandbox.

Por que socket e não `remote: |flatpak run ...` por stdio, como no
flatpak-adv-br: medido em 2026-10-30, abrir o proxy do host custa ~6 ms pelo
socket e ~205 ms pelo stdio, por módulo e por processo consumidor, e o stdio
não permite supervisão. Um `.module` apontando para socket morto (Âncora
fechado, ou morto com SIGKILL) é pulado em 0,9 ms, sem travar e sem afetar o
resto. O custo é o token só aparecer com o serviço rodando.

Cada `.module` sai com `disable-in` para os serviços da sessão GNOME que
carregam todo módulo do usuário (`gsd-smartcard`, `gnome-software`,
`gvfsd-http` e outros): sem isso, o flatpak-adv-br chegou a travar o login no
Fedora 44. O serviço só escreve e apaga arquivos `ancora-*.module`, apaga os
que sobraram de uma queda ao partir, e despublica tudo ao sair. Ele tem uma
instância só (trava em `$XDG_RUNTIME_DIR/ancora/servico.trava`), e os
servidores morrem junto com ele mesmo num SIGKILL.

As extensões são lidas na partida: o Flatpak monta extensões quando o sandbox
começa, e uma instalada depois só aparece num serviço reiniciado.

Ainda falta: host com p11-kit da série 0.25 (Debian trixie, Ubuntu 24.04), que
fala um protocolo diferente do runtime (0.26).

## Os aplicativos que usam os tokens

Um aplicativo Flatpak chega aos tokens pelo `p11-kit-server.socket` do
usuário, que serve os módulos do host (os do Âncora entre eles) e a confiança
do sistema. `src/aplicativos.rs` conhece os aplicativos testados e faz, para
cada um, dois passos, com as receitas que o Tessera testou com token real:

| aplicativo | permissão | banco NSS |
|---|---|---|
| Firefox | `xdg-run/p11-kit/pkcs11` | registra o `p11-kit-client.so` do runtime em cada perfil |
| Chromium, Chrome, Edge, Brave | idem | nada a registrar: o módulo de confiança do runtime já chega ao servidor; tira um registro do p11-kit que duplicaria os tokens |
| Papers, Okular | idem | um banco vazio em `config/mozilla/firefox/ancora.default`, o primeiro que o poppler procura |
| PJeOffice Pro (FlatPark) | já tem | já carrega o cliente |

A permissão é escrita no override do aplicativo (o INI de `flatpak override
--user`), e desfazer tira só essa entrada, e só se foi o Âncora que a pôs (o
registro fica em `concessoes`, nos dados do Âncora). Nunca `--nofilesystem`,
que grava uma negação permanente, nem `--reset`, que apaga o que outros
programas concederam. O Firefox aberto segura um lock no `.parentlock` do
perfil, e o Âncora não mexe no banco nessa hora.

`tests/e2e/aplicativos.sh` prova a cadeia com o NSS do Firefox Flatpak de
verdade: o token aparece uma vez, pelo módulo do Âncora.

Pelo terminal: `ancora aplicativos` (o que falta em cada um),
`ancora configurar <id>` e `ancora desfazer <id>`.

### Navegadores nativos

Fora do Flatpak, o navegador não passa pelo servidor do sistema: carrega o que
está no banco NSS dele. Publicar ali é uma escolha da pessoa, por destino
(`src/nativos.rs`): os perfis do Firefox nativo, e o banco compartilhado do
usuário (`~/.pki/nssdb`, ou `~/.local/share/pki/nssdb` desde o Chromium 146),
que serve Chromium, Chrome, Brave, Edge e o poppler nativo. Publicar registra o
`p11-kit-proxy.so` do host; despublicar tira o registro.

O registro usa o caminho absoluto do proxy, perguntado ao host. Medido em
2026-10-30 no Arch: no host ele carrega e o token aparece uma vez; dentro de um
sandbox, que não tem esse caminho, o NSS o ignora. É isso que impede o token de
aparecer duas vezes no Chromium Flatpak, que lê o mesmo `~/.pki/nssdb`. No
Debian e no Ubuntu o caminho do host (`/usr/lib/x86_64-linux-gnu/`) coincide
com o do runtime, e esse caso ainda duplica: ainda não tratado.

Pelo terminal: `ancora nativos`, e `ancora nativos publicar|despublicar
firefox|sistema`. `tests/e2e/nativos.sh` confere as duas pontas.

### Assinadores

Os assinadores (Lacuna Web PKI, Softplan WebSigner) são programas que a
extensão do navegador abre por native messaging. `src/assinadores.rs` os
publica em todo navegador presente, nativo ou Flatpak, e o assinador roda no
ambiente de quem o chama, como no Sigillum, testado com token real em
29/10/2026:

| navegador | onde o assinador roda | o que o Âncora escreve |
|---|---|---|
| Flatpak | no sandbox do navegador | o manifesto do fabricante (com o `path` trocado) e um lançador em `~/.var/app/<id>/`, links `libaetpkss.so.3` para o `p11-kit-client.so` do runtime, e no override do navegador: ler a pasta da extensão (`:ro`) e o `x11` |
| nativo | no sandbox do Âncora (`ancora assinar <host>`) | o manifesto e um lançador em `~/.local/share/ancora/assinadores/` que chama o `flatpak run` |

O navegador Flatpak não ganha permissão de sair do sandbox, ao contrário do
flatpak-adv-br, que lhe dava o `org.freedesktop.Flatpak`. Os tokens chegam pelo
`libaetpkss.so.3`, um nome que a extensão do navegador já conhece, ligado ao
cliente do p11-kit; o navegador Flatpak precisa da permissão do socket do
sistema, que é a da seção anterior (`ancora configurar <id>`). O programa é
lido direto da instalação da extensão (o caminho com `active` não muda numa
atualização), sem cópia.

Publicar sincroniza: escreve o que falta e apaga o que a publicação anterior
escreveu e não vale mais, inclusive as pastas do Âncora que ficam vazias e as
permissões que só ele deu. Pelo terminal: `ancora assinadores` e
`ancora assinadores publicar|despublicar`. `tests/e2e/assinadores.sh` confere,
com um assinador de teste, que o token aparece uma vez no sandbox do Âncora e
no do Firefox de verdade.

### Um token, exatamente uma vez: verificado, não suposto

As receitas acima são o padrão, não a garantia. O que cada consumidor enxerga
depende da distribuição (o Fedora pode já pôr o proxy no NSS do sistema; no
Debian o caminho do proxy do host coincide com o do runtime), então
`src/verificacao.rs` mede. Publica por alguns segundos o token-sentinela
(`sentinela/sentinela.c`, um módulo PKCS#11 mínimo com um token "Âncora:
verificação", sem PIN e sem objetos), abre o banco NSS do consumidor do jeito
que ele abre, dentro do ambiente dele (o sandbox do aplicativo, pelo
`flatpak run --command=python3`, ou o host), e conta quantas vezes o
sentinela aparece. Tenta, em ordem, não registrar nada e cada forma de
registro, e fica com a primeira que dá 1. Se nem sem o Âncora der 1, diz por
quais módulos o token duplica. Se não der para medir (aplicativo sem
`python3`, por exemplo), aplica a receita padrão e marca "não verificado".

O resultado de cada banco fica em `verificados`, nos dados do Âncora, e
"pronto" quer dizer "verificado, e o banco continua como foi verificado". Cada
dono tem o seu bloco no `pkcs11.txt`: `Âncora (p11-kit)` para os aplicativos
Flatpak e `Âncora (sistema)` para os nativos, porque o `~/.pki/nssdb` é lido
pelos dois.

O módulo do sentinela vai com `priority: 1000`. O `p11-kit server` (o
`p11-kit-server.socket`, por onde os aplicativos Flatpak chegam aos tokens)
abre uma sessão em cada token que lista e encerra a lista inteira no primeiro
`C_OpenSession` que falhar: todo módulo que vem depois some (p11-kit 0.26.5,
`iter.c`; o `main` ainda é assim). Os módulos são ordenados por `priority` e
depois pelo nome, e o sentinela na frente não pode ser escondido.

## A janela

A janela (`src/janela.rs`) e o serviço são processos à parte, e podem estar em
instâncias diferentes do sandbox (o serviço do início automático começa antes
de qualquer janela). Eles conversam por arquivos (`src/estado.rs`):

| o quê | onde | quem escreve |
|---|---|---|
| o que cada driver está fazendo | `$XDG_RUNTIME_DIR/ancora/estado.json` | o serviço, a cada mudança, de uma vez (escreve ao lado e renomeia) |
| se o serviço está vivo | a trava `servico.trava` | ninguém: a janela tenta a trava compartilhada, e o arquivo de estado que sobra de um SIGKILL não engana |
| os drivers desligados | `drivers-desligados`, nos dados | a janela; o serviço confere a cada passo (200 ms) e para ou sobe o driver |
| pedido de parada | `$XDG_RUNTIME_DIR/ancora/parar` | a janela; o serviço o consome e sai como num SIGTERM |

Os dados do Âncora, no sandbox, são a `~/.local/share/ancora` do host: a
permissão `xdg-data/ancora` a monta por cima da pasta de dados do aplicativo.
Nos testes, `ANCORA_DADOS` os redireciona (o Flatpak sobrescreve o
`XDG_DATA_HOME`).

Abrir a janela sobe o serviço, num grupo de sessão próprio, e ele segue depois
de ela fechar. As extensões são lidas quando o sandbox começa, então uma
extensão instalada com o serviço rodando só vale num serviço novo: a linha
dela oferece reiniciar, que pede a parada do serviço antigo (que pode ser de
outra instância, onde um sinal não chega, porque cada instância tem o seu
espaço de PIDs) e sobe um nesta. "Iniciar com a sessão" pede ao portal
Background que rode `ancora servir` no login. No Hyprland e no Sway o portal
não tem essa interface, e a janela diz isso; no GNOME ainda não foi testado.

Os certificados de cada token vêm de `ancora certificados <socket>`, num
processo à parte com limite de 15 s, pelo socket do driver e sem login. A
janela pede de novo só quando a lista de tokens de um driver muda, para não
abrir sessão no cartão a cada sonda. Do X.509 saem o nome comum do titular
(`NOME:CPF` ou `RAZÃO:CNPJ` na ICP-Brasil), o do emissor e o fim da validade,
lidos por um leitor DER mínimo (`src/certificado.rs`).

O que demora (perguntar ao host, medir com o sentinela) roda fora da thread da
interface, uma operação por vez: o interruptor só muda quando ela termina, e
os outros esperam esmaecidos. O que custa ler é relido ao abrir a janela,
quando ela volta ao foco e depois de cada operação; o estado do serviço, a
cada segundo. Desligar um assinador sincroniza a publicação (escreve o que os
ligados pedem e apaga o resto), e abrir a janela sincroniza de novo, para
alcançar um navegador instalado depois.

## Instalar extensões de dentro do aplicativo

A tela de extensões procura, nos remotes que a pessoa já tem configurados, os
refs que estendem o Âncora, e instala, atualiza ou remove pelo
`flatpak-spawn --host flatpak ...`. Isso exige
`--talk-name=org.freedesktop.Flatpak`, que o Flathub marca como saída do
sandbox. A escolha foi consciente: o Âncora é distribuído pelo remote próprio, e
a alternativa (mandar a pessoa para a loja do sistema) tira a instalação da
janela.

As extensões trazem só a receita, e o pacote do fabricante é baixado na
máquina de quem usa, com o sha256 conferido. As licenças dos drivers permitem
usar e guardar uma cópia, não redistribuir. Quem baixa é o Flatpak, na
instalação (`extra-data`), ou o próprio Âncora (`[pacote]`, `src/pacote.rs`),
quando o servidor do fabricante tem cadeia TLS incompleta ou limita cada
conexão. Os dois rodam o mesmo `apply_extra` da extensão, sem rede, e o
Âncora o roda num sub-sandbox que só enxerga a pasta do pacote
(`docs/extensoes.md`).

`src/loja.rs` faz o trabalho pelo `flatpak` do host, sempre com `LC_ALL=C`: as
chaves do `--json` saem traduzidas conforme o idioma (`id_de_aplicativo`), e
uma lista vazia não imprime nem `[]`. O botão `+` da seção Extensões abre o
diálogo de busca (`src/dialogo_extensoes.rs`), com as extensões por família.
Uma extensão oferecida por mais de um remote aparece uma vez, pela do usuário,
que instala sem pedir senha. Remotes desligados e os `no-enumerate` (os que o
flatpak-builder cria) ficam de fora, e um remote fora do ar não esconde os
outros.

O nome e a descrição vêm do appstream que o cliente guardou do remote. Num
remote recém-adicionado ele ainda não existe, e o Flatpak dá como nome o último
pedaço do id. Quando isso acontece com uma extensão do Âncora, o Âncora
atualiza o appstream só daquele remote e lista de novo: atualizar todos a cada
abertura não dá, porque o do Flathub levou 133 s (medido em 2026-10-30).

Instalar, atualizar e remover mudam o host, não este sandbox: as extensões
são montadas quando ele começa. A janela compara o que está montado (o
`app-extensions` do `/.flatpak-info`, com o commit inteiro) com o que está
instalado (`flatpak list`, com o commit abreviado em 12 caracteres) e, se
diferem, mostra um banner com "Reiniciar". Isso vale também para o que mudou por
fora (pelo terminal, ou pela loja do sistema). Reiniciar para o serviço, deixa
no host um processo solto pelo `setsid -f` e sai. Esse processo espera o nome
`dev.lukakuuhaku.Ancora` sair do D-Bus da sessão (senão a janela nova só
repassaria a ativação à velha) e roda `flatpak run`. A janela nova sobe o
serviço novo, que monta as extensões como estão. Medido: 1,2 s até a janela
nova. Duas armadilhas desse caminho:

- nada da saída do processo do host pode voltar por um pipe: ler um pipe que
  ele herdou esperaria o fim dele, que espera o fim da janela;
- o `flatpak ps` não serve para esperar a instância acabar: ele segue listando
  instâncias mortas.

Atualizar ou remover uma extensão apaga os arquivos da versão montada, mesmo
com o sandbox vivo (Flatpak 1.18.2): a pasta em `/app/extensoes` fica vazia.
Por isso as extensões montadas são lidas uma vez por processo
(`extensao::montadas`): a janela não perde a linha, e publicar os assinadores
depois de atualizar um não o despublica. O serviço já tinha o driver carregado
e segue com ele, mas um driver removido dá erro na conexão seguinte, porque o
`p11-kit server` carrega o módulo a cada conexão. Uma extensão removida fica
esmaecida até o reinício.

Uma extensão de um remote do sistema vai para a instalação do sistema, e o
Âncora, mesmo instalado no usuário, a monta: medido em 2026-10-30, instalando
e removendo o SafeSign por um remote `--system`. O Flatpak libera o grupo
`wheel` sem senha (`org.freedesktop.Flatpak.rules`), e por isso a janela do
polkit não pôde ser vista aqui; o Âncora chama o `flatpak` com `-y` e sem
`--noninteractive`, que desligaria a autenticação interativa. Falta conferir
numa VM, com um usuário fora do grupo de administração.

Pelo terminal: `ancora disponiveis`, `ancora instalar|atualizar|remover
Driver.SafeSign`. `tests/e2e/loja.sh` confere o caminho inteiro com um remote
local.

## Compatibilidade com o PJeOffice Pro do FlatPark

O `br.jus.cnj.PJeOfficePro` do FlatPark já tem
`--filesystem=xdg-run/p11-kit/pkcs11` e carrega o `p11-kit-client.so` do
runtime. Basta que os módulos do Âncora estejam publicados no p11-kit do host e
que o `p11-kit-server.socket` do usuário esteja ligado. A extensão própria de
PJeOffice, quando existir, convive com ele.

## Plano de construção

1. **Fundação**: este esqueleto, os gates e a CI.
2. **Contrato de extensão** (feito): o `ancora.toml`, os pontos de extensão
   no manifesto e `ancora extensoes`.
3. **Núcleo de publicação** (feito): PKCS#11 por socket com supervisão,
   aplicativos Flatpak e navegadores nativos (permissão e NSS, verificados com
   o token-sentinela) e os assinadores por native messaging.
4. **Interface base** (feito): os tokens e os certificados, as extensões
   instaladas, os aplicativos e navegadores, e o serviço com a sessão.
5. **Tela de extensões** (feito): busca, instala, atualiza e remove nos
   remotes, e reinicia para valer.
6. **Migração**: as extensões do flatpak-adv-br (SafeSign primeiro), o remote e
   o `end-of-life-rebase` do `dev.lukakuuhaku.AdvBr` para o Âncora.

## Backlog

O que ficou para depois, com o plano, está em [backlog.md](backlog.md).

## Pendências de nome

O repositório vai substituir o `flatpak-adv-br` quando estiver pronto. Ao
renomear, troque as URLs de `data/dev.lukakuuhaku.Ancora.metainfo.xml`, que
hoje apontam para o repositório antigo (o GitHub redireciona até lá).
