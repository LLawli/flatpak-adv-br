# Drivers de token como extensões

> As versões, as URLs e os remendos de cada driver aqui vêm do
> [distrobox-adv-br](https://github.com/pedrohqb/distrobox-adv-br), de Pedro
> HQB, que fez o trabalho de descobrir o que funciona com o quê. O que este
> repositório acrescenta é o empacotamento.


O pacote base traz só o que pode ser redistribuído: o OpenSC, que é software
livre e reconhece boa parte dos cartões e tokens ICP-Brasil. Os drivers
proprietários (SafeSign, SafeNet, SerproID) têm licenças que permitem ao
licenciado usar e guardar uma cópia de backup, **não redistribuir**. Publicar um
pacote com o driver dentro seria redistribuir; um manifesto que você constrói na
sua máquina, baixando da URL do próprio fabricante, não é.

Por isso cada driver é uma **extensão Flatpak** separada:

```sh
./instalar.sh --with-safesign   # token GD Burti, o mais comum na advocacia
./instalar.sh --with-safenet    # eToken 5100, 5110, IDPrime
./instalar.sh --with-serproid   # certificado em nuvem do Serpro
./instalar.sh --with-drivers    # os três
```

ou, se preferir o Makefile:

```sh
make driver-safesign
make driver-safenet
make driver-serproid
make drivers-desinstalar
```

Nada mais é preciso: o pacote varre as extensões instaladas a cada execução, e
o `./host/publicar.sh` republica os módulos para os navegadores. Depois de
instalar ou remover um driver, rode-o de novo.

## Outros pacotes usam estas mesmas extensões

O ponto de extensão `io.github.llawli.AdvBr.Driver` é **público**: qualquer
Flatpak pode declará-lo e receber os drivers que você já instalou aqui, sem
construir nada de novo. É o que o
[PjeOffice-flatpak](https://github.com/LLawli/PjeOffice-flatpak) faz, e por
isso ele não tem mais extensões próprias.

Para um pacote consumir estas extensões, o manifesto dele declara:

```yaml
add-extensions:
  io.github.llawli.AdvBr.Driver:
    directory: lib/pkcs11/drivers
    subdirectories: true
    no-autodownload: true
    autodelete: false
    version: master
```

e o lançador dele varre `lib/pkcs11/drivers/*/` como o daqui: `lib/` no
`LD_LIBRARY_PATH`, `preparar.sh` antes de qualquer carga, `pkcs11/*.so`
registrados no p11-kit do sandbox.

Três detalhes que isso exige saber:

- **O Flatpak não exige que o prefixo da extensão seja o id de quem a declara.**
  Ele procura, entre as refs instaladas, as que começam com aquele nome, e
  monta. O `[ExtensionOf]` gravado na extensão diz de quem ela é; não restringe
  quem pode montá-la. Verificado montando estas extensões dentro do
  `br.jus.cnj.PJeOffice`.
- **`autodelete: false` do lado de quem consome.** Com `true`, desinstalar
  aquele aplicativo levaria junto um driver que este pacote e os navegadores
  também usam.
- **O diretório precisa existir no pacote que consome**, ou não há onde montar.

O ganho é direto: o SerproID sozinho passa de 280 MB, e antes disso havia duas
cópias do mesmo binário no disco, uma por pacote.

## A convenção que uma extensão precisa seguir

O prefixo de build é `/app/lib/pkcs11/drivers/<Nome>`, e dentro dele:

| caminho | o que é |
|---|---|
| `pkcs11/*.so` | os módulos PKCS#11. Cada um vira um `.module` do p11-kit com o nome `<Nome>-<arquivo>`, e um `.module` publicado no host. |
| `lib/` | bibliotecas de apoio de que o driver dependa. Entra no `LD_LIBRARY_PATH` antes de qualquer carga. |
| `preparar.sh` | opcional, executável. Roda antes de o módulo ser carregado, para o que o driver exigir do ambiente. |
| `bin/*` | opcional. Ferramentas da extensão, alcançadas por `flatpak run --command=adv-br-ferramentas io.github.llawli.AdvBr <nome>`. |
| `atalhos/<nome>.desktop` | opcional. Vira atalho de menu no host, escrito pelo `./host/publicar.sh`. Use `@EXEC@` e `@ICONE@`, que ele substitui. |
| `atalhos/<nome>.png` | opcional, o ícone do atalho acima. |

O `.desktop` sai da extensão e vai para `~/.local/share/applications` porque o
Flatpak **não** exporta arquivos de extensão: ele exporta o que está no
aplicativo, no momento em que o aplicativo foi construído. Uma extensão que a
pessoa instala depois nunca passaria por lá.

`preparar.sh` existe por causa do SerproID: a `libneoidp11.so` faz `readdir` em
`~/.config/serproid/certificados` assim que é carregada e derruba com SIGSEGV
quem a carregou se o diretório não existir. Como quem carrega os módulos é o
assinador, um driver quebrado impediria assinar **com qualquer outro token**.

## O que cada driver exige do ambiente

- **SafeSign**: `libgdbm_compat.so.4`, que o runtime não traz. É a interface
  antiga do gdbm, e só existe com `--enable-libgdbm-compat` — daí o módulo
  `gdbm` no manifesto da extensão.
- **SafeNet**: procura `/etc/eToken.conf` e `/etc/eToken.common.conf` por
  caminho absoluto, sem forma de redirecionar. No sandbox `/etc` é tmpfs
  gravável, então o `preparar.sh` copia os arquivos no início de cada execução.
  Também quer `/var/tmp/eToken.cache`, e abre as demais bibliotecas por
  `dlopen` — todas precisam estar no `LD_LIBRARY_PATH`.
- **SerproID**: a `libserproidp11.so` usa símbolos da `libgcc_s`
  (`_Unwind_Resume_or_Rethrow`) e **não a declara** em `DT_NEEDED`. Quem a abre
  com `dlopen` — o p11-kit, o assinador, o PJeOffice — falha com
  `undefined symbol`, a menos que alguma outra coisa já tenha trazido a
  `libgcc_s` para aquele processo. O manifesto corrige com
  `patchelf --add-needed`, o que faz a biblioteca se bastar em qualquer
  processo em vez de depender de sorte.

  Ele também não fala com token físico. O aplicativo autentica na nuvem do
  Serpro e grava cada certificado como um `.cer` em
  `~/.config/serproid/certificados`; a biblioteca lê esse diretório. Por isso a
  extensão traz o aplicativo inteiro, e a primeira coisa a fazer depois de
  instalá-la é abri-lo:

  ```sh
  make serproid
  ```

## Verificação que cada manifesto faz

Cada extensão confere, ainda no build, duas coisas diferentes:

1. que a biblioteca **resolve** as dependências que declara
   (`ldd | grep 'not found'`);
2. que ela **carrega de verdade** (`ctypes.CDLL`).

A segunda existe porque a primeira não bastou: o `ldd` lista o que falta em
`DT_NEEDED` e não vê símbolo indefinido. Foi assim que o driver do SerproID
passou no build e falhou só na hora de ser carregado — que é, na prática, a
hora de assinar, com o token na mão.
