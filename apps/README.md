# Aplicativos como extensões

Aqui ficam os programas que não são nem driver nem assinador de navegador: eles
têm janela própria e o usuário os abre.

| extensão | o que é | tamanho |
|---|---|---|
| `--with-pjeoffice` | PJeOffice Pro, o assinador do CNJ | ~296 MB |

```sh
./instalar.sh --with-pjeoffice
```

Quase todo esse tamanho é o Java que o PJeOffice exige, e é por isso que ele é
extensão: quem não usa o PJe não deve baixar 300 MB para descobrir isso.

Depois de instalado, ele **aparece no menu** — o `./host/publicar.sh` leva o
atalho para o host. Pela linha de comando:

```sh
flatpak run --command=adv-br-ferramentas io.github.llawli.AdvBr pjeoffice-pro
```

Para encerrá-lo com o shutdown hook rodando, `flatpak kill
io.github.llawli.AdvBr` — e não `pkill -f pjeoffice-pro.jar`, que casa a linha
de comando de quem está rodando o `pkill`.

## A convenção que uma extensão de aplicativo segue

O prefixo de build é `/app/lib/apps/<Nome>`, e dentro dele:

| caminho | o que é |
|---|---|
| `bin/<nome>` | o lançador, alcançado por `adv-br-ferramentas <nome>`. |
| `atalhos/<nome>.desktop` | vira atalho de menu no host, escrito pelo `./host/publicar.sh`. Use `@EXEC@` e `@ICONE@`, que ele substitui. |
| `atalhos/<nome>.png` | o ícone do atalho. |
| `lib/` | bibliotecas de apoio, se houver. |
| `preparar.sh` | opcional, roda antes do aplicativo subir. |

O `.desktop` sai da extensão e vai para `~/.local/share/applications` porque o
Flatpak **não** exporta arquivos de extensão: ele exporta o que está no
aplicativo, no momento em que o aplicativo foi construído. Uma extensão que a
pessoa instala depois nunca passaria por lá — e um atalho que aparecesse sem a
extensão instalada seria pior que nenhum.

## O que se sabe do PJeOffice

- Ele descobre driver por `PKCS11_DRIVER` lido como **diretório**, de onde
  carrega um `pkcs11.so`. É um caminho só, e por isso o pacote base traz o shim
  (`src/pkcs11-shim.c`), que repassa ao p11-kit-proxy e responde por todos os
  drivers de uma vez.
- O `signer4j` canoniza o caminho com `toRealPath()` antes de gravá-lo em
  `~/.pjeoffice-pro/pjeoffice-pro.config`. Por isso o shim é um arquivo
  regular, e não um symlink: senão o que ficaria gravado seria
  `libp11-kit.so.0.4.10`, que some na próxima atualização de runtime.
- Ele escuta em `*:8800` e `*:8801`, não em loopback. É dele, não do
  empacotamento.
- A lista de origens que ele aceita está dentro do `.jar`, em `preflight.list`:
  `https://*.jus.br`, `*.mp.br`, `*.gov.br`, `*.def.br` e o próprio localhost.
