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
│   sonda a cada 10 s (ancora sondar, num processo à parte)  │
└────────────────────────────────────────────────────────────┘
        ▲ remote: unix:path=... em ~/.config/pkcs11/modules/ancora-<driver>.module
┌──────── host ──────────────────────────────────────────────┐
│ p11-kit-proxy (aplicativos nativos)                        │
│ p11-kit-server.socket do usuário (aplicativos Flatpak com  │
│   --filesystem=xdg-run/p11-kit/pkcs11, como o PJeOffice)   │
└────────────────────────────────────────────────────────────┘
```

`ancora servir` (`src/servico.rs`) sobe um `p11-kit server` por módulo
PKCS#11 das extensões de driver e o sonda a cada 10 s, num processo à parte
com limite de 10 s. Só o que respondeu da última vez fica publicado
(`src/publicacao.rs`). Um driver que não responde sai da publicação e é
reiniciado, e depois de 3 vezes seguidas é parado (`src/supervisao.rs`): um
driver travado e publicado travaria todo aplicativo que abrisse o p11-kit.

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
fala um protocolo diferente do runtime (0.26), e o início automático do
serviço com a sessão.

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

## Instalar extensões de dentro do aplicativo

A tela de extensões procura, nos remotes que a pessoa já tem configurados, os
refs que estendem o Âncora, e instala, atualiza ou remove pelo
`flatpak-spawn --host flatpak ...`. Isso exige
`--talk-name=org.freedesktop.Flatpak`, que o Flathub marca como saída do
sandbox. A escolha foi consciente: o Âncora é distribuído pelo remote próprio, e
a alternativa (mandar a pessoa para a loja do sistema) tira a instalação da
janela.

As extensões usam `extra-data`: o remote publica só a receita, e o pacote do
fabricante é baixado na máquina de quem instala, com o sha256 conferido. As
licenças dos drivers permitem usar e guardar uma cópia, não redistribuir.

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
3. **Núcleo de publicação**: PKCS#11 (feito: socket por driver, com
   supervisão), aplicativos (permissão e NSS) e native messaging.
4. **Interface base**: o estado do certificado e as extensões instaladas.
5. **Tela de extensões**: busca, instala e atualiza nos remotes.
6. **Migração**: as extensões do flatpak-adv-br (SafeSign primeiro), o remote e
   o `end-of-life-rebase` do `dev.lukakuuhaku.AdvBr` para o Âncora.

## Pendências de nome

O repositório vai substituir o `flatpak-adv-br` quando estiver pronto. Ao
renomear, troque as URLs de `data/dev.lukakuuhaku.Ancora.metainfo.xml`, que
hoje apontam para o repositório antigo (o GitHub redireciona até lá).
