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
3. **Núcleo de publicação**: PKCS#11, NSS e native messaging. O mecanismo da
   ponte será escolhido por medição: `remote: |flatpak run ...` por stdio, como
   no flatpak-adv-br, ou um `p11-kit server` por driver num socket, como no
   Tessera, que também despublica um driver travado.
4. **Interface base**: o estado do certificado e as extensões instaladas.
5. **Tela de extensões**: busca, instala e atualiza nos remotes.
6. **Migração**: as extensões do flatpak-adv-br (SafeSign primeiro), o remote e
   o `end-of-life-rebase` do `dev.lukakuuhaku.AdvBr` para o Âncora.

## Pendências de nome

O repositório vai substituir o `flatpak-adv-br` quando estiver pronto. Ao
renomear, troque as URLs de `data/dev.lukakuuhaku.Ancora.metainfo.xml`, que
hoje apontam para o repositório antigo (o GitHub redireciona até lá).
