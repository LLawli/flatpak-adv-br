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

## Créditos

A parte difícil deste problema não é o Flatpak: é descobrir **qual** versão de
qual driver funciona com qual token, quais bibliotecas antigas cada um exige, e
que remendo faz cada um deles rodar numa distribuição atual. Esse trabalho é do
Pedro HQB, no
[distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br) — as URLs, as
versões e os contornos que estão nos manifestos de extensão deste repositório
vêm de lá, através do [sora-adv-br](https://github.com/LLawli/sora-adv-br), que
é um fork dele.

Sem esse levantamento, nada aqui existiria: empacotar é fácil quando alguém já
respondeu o que empacotar.

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

## O que vem, e o que você escolhe

O **pacote base** tem poucos megabytes e traz o que todo mundo usa: o OpenSC,
que já reconhece parte dos cartões e tokens ICP-Brasil, e a ponte que leva o
token aos navegadores, ao Papers e aos aplicativos.

Todo o resto é extensão, instalada quando você for usar — e pode ser depois:

| opção | o que traz | tamanho |
|---|---|---|
| `--with-safesign` | driver do token GD Burti, o mais usado na advocacia | ~2 MB |
| `--with-safenet` | driver dos eToken 5100, 5110 e IDPrime | ~30 MB |
| `--with-serproid` | certificado em nuvem do Serpro, com o aplicativo dele | ~288 MB |
| `--with-webpki` | Lacuna Web PKI, para assinar em navegador | ~142 MB |
| `--with-websigner` | Softplan WebSigner, dos sistemas SAJ | ~140 MB |
| `--with-certisign` | Certisign WebSigner, do portal da OAB | ~2 MB |
| `--with-pjeoffice` | PJeOffice Pro, para assinar no PJe (CNJ) | ~296 MB |
| `--with-drivers`, `--with-assinadores`, `--with-tudo` | os grupos | |

Nada disso vem embutido por dois motivos que se somam. **Licença**: SafeSign,
SafeNet, SerproID e os assinadores permitem ao licenciado usar e guardar uma
cópia de backup, não redistribuir — cada extensão baixa da URL do próprio
fabricante, na sua máquina. E **tamanho**: quem não usa o PJe não deveria
baixar 300 MB de Java para descobrir isso.

Como se escreve uma extensão nova está em
[drivers/README.md](drivers/README.md),
[assinadores/README.md](assinadores/README.md) e [apps/README.md](apps/README.md).

## VIDaaS: em desenvolvimento, e precisando de testadores

O **VIDaaS**, certificado em nuvem da Valid, ainda não está aqui — mas não é
caso perdido, e a razão é interessante.

Não existe biblioteca PKCS#11 para Linux: o VIDaaS Connect, que é quem faz a
ponte, só tem versão para Windows e macOS. Só que a chave privada do VIDaaS
**não mora no seu computador** — ela fica num HSM na nuvem da Valid, e toda
assinatura é uma chamada à API deles, aprovada no aplicativo do celular. Ou
seja: não há nada de específico de Windows no que importa. Falando a mesma API,
o Linux funciona.

O trabalho está em andamento e a parte que dá para exercitar sem certificado já
funciona (autorização e QR Code). **O que falta precisa de um certificado
VIDaaS de verdade**: descobrir o certificado, assinar e conferir a assinatura
resultante — três passos que ninguém consegue testar sem ter um em mãos.

**É aqui que você pode ajudar.** Se você tem um certificado VIDaaS ativo e topa
rodar um roteiro de teste (leva alguns minutos, e nenhuma chave sai do seu
poder — a aprovação continua sendo no seu celular), escreva para:

> **contato@lukakuuhaku.dev**

Com testadores, o VIDaaS entra como mais uma extensão, do mesmo jeito que os
outros.

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

Um comando, sem clonar nada:

```bash
curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/install.sh | sh
```

Isso instala o pacote base e o publica para os navegadores. Com o driver do seu
token e o assinador que o seu tribunal usa, que é o caso comum:

```bash
curl -fsSL https://raw.githubusercontent.com/LLawli/flatpak-adv-br/main/packaging/install.sh | sh -s -- --with-safesign --with-webpki
```

Se preferir olhar o código antes — e você deveria, é um script que constrói
pacotes na sua máquina:

```bash
git clone https://github.com/LLawli/flatpak-adv-br
cd flatpak-adv-br
./instalar.sh --with-safesign --with-webpki
```

### O que fica no seu sistema

| onde | o quê |
|---|---|
| `~/.local/share/flatpak-adv-br` | o código, que você usa depois para publicar e diagnosticar (centenas de KB) |
| os Flatpaks | o pacote base e as extensões que você pediu |
| sua home | os arquivos de configuração que o `publicar.sh` escreve |

**Nenhum artefato de construção.** Eles são feitos em
`~/.cache/flatpak-adv-br/construcao` e apagados no fim — o cache do `flatpak-builder` de
uma instalação completa passa de 1 GB. Quem clona à mão e roda `./instalar.sh`
mantém esse cache (que é o que torna a próxima construção rápida); para apagá-lo,
`./instalar.sh --limpar` ou `make limpar`.

**Rodar de novo acrescenta.** Instalou o driver hoje e amanhã descobriu que
precisa do PJeOffice? `./instalar.sh --with-pjeoffice` — o que já está pronto
não é refeito, e nada do que você tinha é perdido. O comando é idempotente:
pode ser repetido à vontade.

```bash
./instalar.sh --with-tudo         # tudo, se você prefere não escolher
./instalar.sh --refazer           # reconstrói o que já está instalado
./instalar.sh --sem-publicar      # só constrói; não toca em nada fora
./instalar.sh --conceder          # já concede as permissões dos navegadores Flatpak
./instalar.sh --ajuda
```

Se algo falhar no meio, corrija a causa e rode de novo: o instalador continua
de onde parou, e uma extensão que falhe não impede as outras.

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

**GPL-3.0-only**, em [LICENSE](LICENSE) — a versão 3 da GPL, sem a cláusula de
"ou qualquer versão posterior". A mesma do
[distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br), de onde vem o
trabalho de garimpo dos drivers.

Isso vale para o empacotamento, que é o que este repositório contém. **Não**
vale para o que ele instala: os drivers de token e os assinadores pertencem aos
seus fabricantes e seguem as licenças deles, e nenhum binário proprietário é
redistribuído aqui — todos são baixados da fonte original no momento da
construção.
