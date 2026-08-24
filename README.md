# flatpak-adv-br

Certificado digital para a advocacia brasileira **em Flatpak**, usado nos
navegadores que você já tem.

Os drivers do token (SafeSign, SafeNet, SerproID) e os assinadores (Lacuna Web
PKI, Softplan WebSigner, Certisign WebSigner) ficam dentro de um Flatpak. O que
muda em relação ao resto do gênero: **nenhum navegador é instalado**. O Firefox,
o Chrome, o Brave ou o Vivaldi que você já usa continuam sendo os seus,
inclusive se forem Flatpak, e passam a enxergar o token através deste pacote.
O Papers, para assinar e validar PDF fora do navegador, também.

É o [sora-adv-br](https://github.com/LLawli/sora-adv-br) com Flatpak no lugar do
distrobox. A ideia é a mesma; o que muda é o veículo.

## A ideia

Um contêiner resolve o problema dos drivers e cria outro: o navegador que
enxerga o token seria o de dentro, e passar a viver nele significa perder
senhas, extensões, abas e perfil. A saída não é levar o navegador para dentro,
é publicar o que está dentro para o navegador de fora. São dois mecanismos
diferentes, porque são dois problemas diferentes:

| O site pede | Quem usa o token | Como atravessa |
|---|---|---|
| **autenticação por certificado** (Projudi, eproc, login do PJe, gov.br) | o próprio navegador, carregando o módulo PKCS#11 dentro do processo dele | um `.module` do p11-kit que inicia `flatpak run` e conversa pelo pipe |
| **assinatura** (SAJ, portal da OAB, Lacuna, PJe) | um programa à parte, que o navegador executa e com quem conversa por stdin/stdout | o manifesto de *native messaging* apontando para um atalho que entra no Flatpak |

Nos dois casos, o que se escreve no host é um arquivo de configuração dentro da
sua própria home. Nada é instalado no sistema, nada precisa de `sudo`.

## O que vem no pacote

| | |
|---|---|
| **OpenSC** | cobre boa parte dos cartões e tokens ICP-Brasil, e é software livre |
| **Lacuna Web PKI** | assinar em sistemas que usam o componente da Lacuna |
| **Softplan WebSigner** | assinar nos sistemas SAJ |
| **Certisign WebSigner** | portal de assinatura eletrônica da OAB |
| **PJeOffice Pro** | assinar no PJe (CNJ). Tem atalho próprio no menu |

Os drivers proprietários entram como **extensões**, que você constrói com uma
opção do instalador:

| Opção | Para que serve |
|---|---|
| `--with-safesign` | token GD Burti, o mais usado na advocacia |
| `--with-safenet` | eToken 5100, 5110, IDPrime |
| `--with-serproid` | certificado em nuvem do Serpro (traz também o aplicativo) |
| `--with-drivers` | os três |

**Instale só o driver do token que você usa.** Cada driver instalado é um
módulo a mais que o navegador enumera ao abrir, e um deles cobra caro por isso:
o SafeNet, sem token SafeNet espetado, leva mais de um minuto. A seção 6 do
`./diagnostico.sh` cronometra cada um e avisa.

Eles não vêm no pacote porque as licenças de SafeSign, SafeNet e SerproID
permitem ao licenciado usar e guardar uma cópia de backup, **não redistribuir**.
Um manifesto que você constrói na sua máquina, baixando da URL do fabricante,
não é redistribuição. Ver [drivers/README.md](drivers/README.md).

Instalado o SerproID, o aplicativo dele **aparece no menu** — é por ele que se
associa o certificado da nuvem antes de assinar. Pela linha de comando:

```bash
flatpak run --command=adv-br-ferramentas io.github.llawli.AdvBr serproid
```

O **PJeOffice Pro** vem junto e aparece no menu com atalho próprio. Ele também
existe como pacote separado, em
[PjeOffice-flatpak](https://github.com/LLawli/PjeOffice-flatpak), para quem quer
só o assinador do PJe — aquele pacote **usa estas mesmas extensões de driver**.
Ter os dois instalados funciona, mas é redundante: são o mesmo assinador, a
mesma configuração em `~/.pjeoffice-pro` e a mesma porta 127.0.0.1:8800, e o
menu mostra duas entradas iguais. Escolha um.

Instale o driver uma vez e os navegadores, o Papers e o PJeOffice enxergam o
mesmo token, com uma cópia só no disco. Como outro pacote consome estas
extensões está em [drivers/README.md](drivers/README.md).

O **VIDaaS**, certificado em nuvem da Valid, não entra: não existe biblioteca
para Linux.

## Requisitos

O daemon do PC/SC roda **no host**, não no sandbox: é ele quem fala com a
leitora USB. Dentro do Flatpak existe só a biblioteca cliente.

```bash
# Fedora
sudo dnf install flatpak flatpak-builder pcsc-lite pcsc-lite-ccid p11-kit p11-kit-server

# Fedora atômico (Silverblue, Kinoite): flatpak já vem
sudo rpm-ostree install pcsc-lite pcsc-lite-ccid

# Debian, Ubuntu, Mint
sudo apt install flatpak flatpak-builder pcscd libccid p11-kit

# Arch, Manjaro, CachyOS, EndeavourOS
sudo pacman -S flatpak flatpak-builder pcsclite ccid p11-kit

# openSUSE Tumbleweed
sudo zypper in flatpak flatpak-builder pcsc-ccid p11-kit
```

```bash
sudo systemctl enable --now pcscd.socket
```

Sem `flatpak-builder` no sistema, o instalador aceita o do Flathub:
`flatpak install --user flathub org.flatpak.Builder`.

**Debian, Ubuntu e o p11-kit.** A ponte entre o pacote e os navegadores exige
que o p11-kit do host e o do pacote estejam na mesma série. O runtime traz a
0.26; Debian trixie e Ubuntu 24.04 trazem a 0.25, e divergir aí faz o token
aparecer, o PIN ser aceito e **toda assinatura falhar** — inclusive a do login
por certificado. O `./instalar.sh` detecta isso sozinho e compila, dentro do
pacote, um p11-kit da série do seu host; você não precisa fazer nada. Ver
[docs/ARMADILHAS.md](docs/ARMADILHAS.md).

**Não é preciso `nss-tools`.** O registro nos bancos NSS dos navegadores é feito
editando o `pkcs11.txt`, que é o que o `modutil` faria — assim o projeto não
exige um pacote a mais num sistema atômico.

## Instalação

```bash
git clone https://github.com/LLawli/flatpak-adv-br
cd flatpak-adv-br
./instalar.sh
```

Com o driver do seu token, que é o caso comum:

```bash
./instalar.sh --with-safesign          # GD Burti
./instalar.sh --with-safenet           # eToken
```

Opções:

```bash
./instalar.sh --sem-publicar   # só constrói e instala; não toca em nada fora
./instalar.sh --conceder       # já concede as permissões dos navegadores Flatpak
./instalar.sh --refazer        # reconstrói extensões já instaladas
./instalar.sh --ajuda
```

A construção baixa dos sites dos fabricantes: são cerca de 95 MB de assinadores,
mais o que cada driver pedido pesar. O instalador é **idempotente**: se algo
falhar no meio, corrija a causa e rode de novo.

Ao final, confira com:

```bash
./diagnostico.sh
```

## Depois de instalar

**1. Feche os navegadores por inteiro e reabra.** Não basta fechar a janela: o
módulo PKCS#11 e os manifestos são lidos na inicialização do processo.

**2. Se o seu navegador for Flatpak, conceda as permissões** que o instalador
imprimiu:

```bash
systemctl --user enable --now p11-kit-server.socket
flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 org.mozilla.firefox
flatpak override --user --talk-name=org.freedesktop.Flatpak org.mozilla.firefox
```

A primeira dá ao sandbox o socket do p11-kit do host (autenticação); a segunda
autoriza o `flatpak-spawn` a falar com o portal (assinatura). As duas afrouxam o
confinamento do navegador, e é por isso que o projeto as imprime em vez de
executá-las. Se preferir decidir uma vez e não digitar:

```bash
./host/publicar.sh --conceder
```

**3. Instale a extensão de cada assinador que for usar:**

- Lacuna Web PKI, <https://get.webpkiplugin.com/>
- Softplan WebSigner, <https://websigner.softplan.com.br/>
- Certisign WebSigner, <https://get.websignerplugin.com/>

**4. Em cada extensão, abra a aba "Cripto Dispositivos"** e, em *Opções
personalizadas*, no campo *Nome do arquivo SO (com extensão)*, escreva:

```
/pkcs11/adv-br.so
```

e clique no **+**.

Esse caminho responde por **todos** os drivers instalados aqui, inclusive os que
você instalar depois. As opções prontas da extensão ("Tokens SafeNet",
"Dispositivos SafeSign AET") apontam para `/usr/lib`, que dentro do sandbox
pertence ao runtime e não pode receber driver nenhum.

O mesmo caminho vale no **PJeOffice**, se você preferir apontá-lo à mão: o
lançador dele cria os mesmos atalhos. Mas ali não é preciso — o PJeOffice
encontra os drivers sozinho.

## O PJeOffice escuta em todas as interfaces

O assinador do CNJ sobe um servidor em **`*:8800`** (e HTTPS em `*:8801`), não
em loopback — é assim que ele é, não é efeito do empacotamento; `ss -ltn`
mostra. Numa rede que você não controla, bloqueie as duas portas no firewall.

Os domínios que ele aceita como origem estão dentro do `.jar`, em
`preflight.list`: `https://*.jus.br`, `*.mp.br`, `*.gov.br`, `*.def.br`, mais
`http://127.0.0.1:8800` e `https://127.0.0.1:8801`. Quando um sistema estadual
fora desses domínios não conversa com o assinador, é essa lista — não é
empacotamento.

Para encerrá-lo de forma limpa, com o shutdown hook rodando:

```bash
flatpak kill io.github.llawli.AdvBr
```

## Assinar e validar PDF no Papers

O Papers usa o banco NSS da sua home (`~/.pki/nssdb`), e é lá que o
`./host/publicar.sh` registra o módulo — o mesmo arquivo serve ao Papers em
Flatpak e a qualquer programa do host. Falta só a permissão do socket:

```bash
flatpak override --user --filesystem=xdg-run/p11-kit/pkcs11 org.gnome.Papers
```

que o `--conceder` também faz. Depois disso o token aparece na lista de
certificados do Papers na hora de assinar.

## Conferindo

```bash
./diagnostico.sh              # o encanamento inteiro, do token ao navegador
./host/testar-pkcs11.sh       # o token atravessa até o host e até um sandbox?
./host/testar-assinador.sh    # conversa com os assinadores como o navegador faria
./tests/testar.sh             # testes do repositório (não precisam de token)
```

## Desfazer

```bash
./host/publicar.sh --remover   # tira os módulos, manifestos e atalhos
make desinstalar               # e remove o Flatpak
```

As permissões de Flatpak não são revogadas junto: quem não as concedeu não as
tira. O `--remover` imprime os comandos.

## Onde estão as armadilhas

[docs/ARMADILHAS.md](docs/ARMADILHAS.md) reúne o que foi medido: por que
`fallback-x11` não serve, por que o Fedora não tem `p11-kit-client.so`, por que
há um certificado de CA no repositório, e o que o Certisign WebSigner faz de
diferente.

## Licença

GPL-3.0-or-later. Os drivers e assinadores baixados durante a construção
pertencem aos seus fabricantes e seguem as licenças deles.
