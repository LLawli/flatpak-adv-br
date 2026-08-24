# Arquitetura

Este documento é para quem vai mexer no projeto. O que ele resolve, e para quem
usa, está no [README](../README.md); o que já foi medido e enganou está em
[ARMADILHAS.md](ARMADILHAS.md).

## O problema

Um contêiner resolve o problema dos drivers de token e cria outro: o navegador
que enxerga o token passa a ser o de dentro, e mudar de navegador significa
perder senhas, extensões, abas e perfil. A saída não é levar o navegador para
dentro — é publicar, para o de fora, o que está dentro.

São dois mecanismos, porque são dois problemas:

| o site pede | quem usa o token | como atravessa |
|---|---|---|
| autenticação por certificado | o próprio navegador, carregando o módulo PKCS#11 no processo dele | remoting do p11-kit |
| assinatura | um programa à parte, que o navegador executa | native messaging |

## As peças

```
io.github.llawli.AdvBr              pacote base: OpenSC, pcsc-lite, shim, scripts
├── .Driver.<Nome>                  extensões de driver     → lib/pkcs11/drivers/
├── .Assinador.<Nome>               extensões de assinador  → lib/assinadores/
└── .App.<Nome>                     extensões de aplicativo → lib/apps/
```

O pacote base não sabe quais extensões existem: ele **varre** os três
diretórios a cada execução. Acrescentar um driver, um assinador ou um
aplicativo é escrever um manifesto e uma linha na tabela do `instalar.sh` — não
há registro central para atualizar, e é por isso que instalar uma extensão
depois funciona sem tocar no pacote base.

Cada extensão segue a convenção do README da pasta dela.

## A ponte PKCS#11, e o que ela exige

O `./host/publicar.sh` escreve, na home do usuário, um `.module` por driver:

```
remote: |flatpak run --command=adv-br-pkcs11 io.github.llawli.AdvBr <caminho.so>
```

O p11-kit do host inicia esse comando sob demanda e conversa com ele por um
pipe. **Um processo por módulo**, e não um só exportando o proxy do sandbox,
por dois motivos: isolamento (um driver que derrube o processo que o carregou
leva junto só a si mesmo) e porque o `p11-kit-trust.module` que o Flatpak
injeta em todo sandbox é um bind somente-leitura que não sai de lá — exportar o
proxy devolveria ao host, como se fossem tokens, as âncoras de confiança que
ele já tem.

O que trafega no pipe é a tabela de funções PKCS#11 serializada, e as duas
pontas precisam concordar sobre ela. Ver a seção da série do p11-kit em
[ARMADILHAS.md](ARMADILHAS.md), e `packaging/p11kit-series.txt`.

## O que se escreve no host, e onde

Nada é instalado no sistema. Tudo mora na home de quem usa:

| o quê | onde |
|---|---|
| módulos PKCS#11 | `~/.config/pkcs11/modules/adv-br-*.module` |
| atalhos dos assinadores | `~/.local/bin/adv-br-*` |
| atalhos dos assinadores, para navegador em Flatpak | `~/.var/app/<id>/data/adv-br/` |
| manifestos de native messaging | o diretório de cada navegador |
| registro nos bancos NSS | o `pkcs11.txt` de cada perfil |
| atalhos de menu | `~/.local/share/applications/io.github.llawli.AdvBr.*.desktop` |

`./host/publicar.sh --remover` desfaz exatamente isso, e nada além disso.

## Por que sem `nss-tools`

Num banco NSS moderno (cert9.db + key4.db) a lista de módulos não vive dentro
do banco: vive em `pkcs11.txt`, texto puro ao lado dele — é isso que o
`modutil -add` edita. Editar o arquivo direto evita exigir um pacote a mais num
sistema atômico, onde ele custa um `rpm-ostree` e um reboot. Ver
`host/nssdb.py`.

## Provar exige chamar

Contar módulos ou verificar se um socket existe não prova nada: o Flatpak já
põe um socket do p11-kit em todo sandbox, e um p11-kit-proxy sem módulo nenhum
falha igual a um driver quebrado. As provas do repositório chamam de verdade —
`tests/prova-pkcs11.py` e `tests/prova-nss.py` — e nenhuma delas autentica:
listar tokens não exige PIN, e cada tentativa errada gasta uma das poucas que
um token de hardware tem.
