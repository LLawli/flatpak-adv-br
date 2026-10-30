# Extensões: o contrato

Para quem vai empacotar um driver, um assinador ou um aplicativo para o Âncora.
O código que lê este contrato está em `src/extensao.rs`.

## O ref e onde ele é montado

| família | ref | montada em |
|---|---|---|
| driver de token | `dev.lukakuuhaku.Ancora.Driver.<Nome>` | `/app/extensoes/driver/<Nome>` |
| assinador | `dev.lukakuuhaku.Ancora.Assinador.<Nome>` | `/app/extensoes/assinador/<Nome>` |
| aplicativo | `dev.lukakuuhaku.Ancora.App.<Nome>` | `/app/extensoes/app/<Nome>` |

O manifesto da extensão usa `build-extension: true`,
`runtime: dev.lukakuuhaku.Ancora` e `runtime-version: master`, e instala tudo
em `${FLATPAK_DEST}`. O que for parar fora desse prefixo não entra na
extensão. Módulos com sistema de build (autotools, meson) precisam de
`build-options: prefix:` com o lugar de montagem
(`/app/extensoes/driver/<Nome>`): o builder não muda o prefixo numa extensão,
e sem isso eles tentam instalar em `/app`.

O Âncora fornece o `libpcsclite.so.1` (pcsc-lite 2.5.1, em `/app/lib`), que
fala com o `pcscd` do host: um driver não deve trazer o seu.

O repositório de exemplo é o do SafeSign, em `extensoes/Driver.SafeSign`. Os
outros mostram variações: `Driver.SafeNet` (pacote dentro de um zip),
`Driver.SerproID` (`HOME` própria, `LD_PRELOAD` e uma ferramenta),
`Assinador.WebPKI` (manifestos de native messaging).

## Pacote do fabricante

Um driver ou assinador que não pode ser redistribuído entra como receita, e o
pacote é baixado na máquina de quem usa, com o sha256 conferido. Há dois
jeitos, e o `apply_extra` da extensão é o mesmo nos dois:

| | `extra-data` (Flatpak) | `[pacote]` (Âncora, contrato 2) |
|---|---|---|
| quando baixa | na instalação | na primeira vez que a extensão é usada montada |
| cadeia TLS incompleta | falha | completada com o intermediário que a extensão traz |
| conexões | uma | até 8, se o servidor aceitar faixas |
| pacote errado | a instalação falha | a extensão aparece com o motivo, e tenta de novo |

Use `extra-data` por padrão. Use `[pacote]` quando o servidor do fabricante
tem cadeia TLS incompleta (Softplan) ou limita a velocidade de cada conexão
(DigiCert, do SafeNet: 6 minutos numa conexão, menos de 1 minuto em 8).

### `extra-data`

Entra no manifesto como fonte `extra-data` (`filename`,
`url`, `sha256`, `size`): o remote publica só a receita, e o Flatpak baixa o
pacote na máquina de quem instala, confere o sha256 e roda
`${FLATPAK_DEST}/bin/apply_extra`. O que se sabe dele, medido no Flatpak
1.18.2:

- roda sem rede, com o runtime do `[ExtensionOf]` (o GNOME 51) em `/usr`, e com
  o diretório atual em `extra/`, onde o pacote foi baixado; o que ele deixa ali
  é o que a extensão tem em `extra/`;
- o runtime não tem `ar`: o `bsdtar` abre um `.deb` direto
  (`bsdtar -xOf pacote.deb 'data.tar.*' | bsdtar -xf - ...`), e os caminhos
  dentro do `data.tar` começam com `./`;
- o `--install` do flatpak-builder falha nessa etapa ("apply_extra script
  failed, exit status 256"), porque o `org.flatpak.Builder` já é um Flatpak e
  não abre o sandbox do `apply_extra`. Exporte com `--repo` e instale do
  repositório, como quem usa;
- um `apply_extra` quebrado, ou uma biblioteca que o carregador não resolve,
  só apareceria na máquina de quem instala. Repita no build o que ele faz numa
  cópia do mesmo pacote (uma fonte `type: file`, que não entra na extensão) e
  abra cada biblioteca de verdade (`ctypes.CDLL`). O `ldd` não serve: não vê
  símbolo indefinido, e numa `.so` sem bit de execução (como vêm a do SafeSign
  e as do SafeNet) só avisa e não lista nada, então um `grep 'not found'`
  passa sempre;
- quem baixa é o Flatpak de quem instala, com a validação TLS do sistema e sem
  como acrescentar uma CA. Um servidor com cadeia incompleta (o da Softplan)
  torna a extensão impossível de instalar, mesmo que o build passe;
- o sha256 é fixo, e alguns fabricantes servem sempre o pacote mais novo numa
  URL que só parece ter versão (Lacuna e Softplan ignoram o número). Uma
  versão nova quebra a instalação até o manifesto ser atualizado: o
  `scripts/ci.sh` dessas extensões compara o tamanho servido com o `size`;
- o download pode ser lento: o zip do SafeNet (91 MB, da DigiCert) já levou
  6 minutos e já estourou o tempo limite do Flatpak.

### `[pacote]`

```toml
contrato = 2
[pacote]
url = "https://websigner.softplan.com.br/Downloads/2.15.0/webpki-chrome-64-deb"
sha256 = "04fa41e9..."
tamanho = 47128110
arquivo = "websigner.deb"          # o nome com que o apply_extra o recebe
cadeia = "ThawteTLSRSACAG1.pem"    # opcional: intermediários que o servidor não manda
```

O Âncora (`src/pacote.rs`) baixa para
`~/.local/share/ancora/pacotes/<Família>.<Nome>/<sha256>`, confere tamanho e
sha256 e roda `bin/apply_extra` da extensão como o Flatpak rodaria: sem rede,
num sub-sandbox (`flatpak-spawn --sandbox`) que só enxerga essa pasta, com ela
como diretório atual. Com `[pacote]`, os caminhos `extra/...` do descritor
apontam para essa pasta. Um navegador Flatpak ganha permissão de lê-la.

Quem prepara: o serviço, antes de subir um driver (o driver aparece como
"iniciando" e, se falhar, com o motivo, tentando de novo em 5 minutos); a
publicação dos assinadores, antes de escrever os lançadores; e
`ancora preparar [Família.Nome]`, pelo terminal. Pacotes de extensões
removidas e versões anteriores são apagados quando o serviço começa.

A `url` precisa ser HTTPS, e os redirecionamentos também. A cadeia é somada às
CAs do sistema, não as substitui: a conexão continua validada, só que com o
intermediário que o servidor deixou de mandar.

Sem `extra-data`, o manifesto não precisa de nada do pacote. Se o
flatpak-builder conseguir baixá-lo, mantenha a cópia de verificação
(`type: file`, `dest-filename: verificar.*`) para testar o `apply_extra` no
build; se não (Softplan), o `scripts/ci.sh` o testa sobre o pacote de verdade.

### O fabricante mudou o pacote

Cada repositório de extensão tem `scripts/fabricante.sh`, que baixa o pacote
de novo, sem cache, e compara tamanho e sha256 com o que a extensão espera
(`scripts/pacote.py` lê do `[pacote]` ou do `extra-data`). A action
`.github/workflows/fabricante.yml` o roda todo dia, e a falha chega por e-mail.

## O descritor: `ancora.toml`

Fica na raiz da extensão (`${FLATPAK_DEST}/ancora.toml`). Diretório montado
sem ele não é extensão do Âncora e é ignorado.

```toml
contrato = 2                      # obrigatório; 1 serve a quem não usa [pacote]
nome = "SafeSign IC"              # obrigatório
fabricante = "A.E.T. Europe"
descricao = "Tokens GD Burti e outros SafeSign"
icone = "icone.svg"
criar_diretorios = ["~/.config/fabricante"]

[[pkcs11]]                        # drivers: um ou mais
modulo = "extra/usr/lib/libaetpkss.so"
bibliotecas = ["extra/usr/lib"]   # vão para o LD_LIBRARY_PATH
ambiente = { XDG_CONFIG_HOME = "~/.config" }

[[assinador]]                     # assinadores: um ou mais
host = "com.lacunasoftware.webpki"
executavel = "extra/bin/webpki"
manifesto_firefox = "extra/native-messaging/firefox.json"
manifesto_chromium = "extra/native-messaging/chromium.json"
ambiente = {}
pkcs11 = ["libaetpkss.so.3"]      # nomes que a extensão do navegador manda carregar
x11 = true                        # desenha só por X11 (Avalonia)

[[ferramenta]]                    # aplicativos: uma ou mais; opcional nos outros
nome = "Token Administration"
executavel = "extra/usr/bin/tokenadmin"
argumentos = []
ambiente = {}
atalho = true                     # ganha atalho no menu do sistema
```

### Regras

- **Caminhos são relativos à extensão** e não podem sair dela: caminho
  absoluto, `..` ou vazio são recusados. O que o `extra-data` baixa fica em
  `extra/`.
- **`~/` é a home**, e só vale no começo de `criar_diretorios` (obrigatório
  ali) e dos valores de `ambiente`.
- **Cada família exige o que oferece**: driver sem `[[pkcs11]]`, assinador sem
  `[[assinador]]` e aplicativo sem `[[ferramenta]]` são recusados.
- **Campo desconhecido é erro**, para que um erro de digitação não vire um
  campo ignorado em silêncio.
- **`contrato` maior que o deste Âncora** aparece como "feita para uma versão
  mais nova do Âncora", mesmo que traga campos que ele não conhece. Um campo
  novo no contrato aumenta esse número.
- **Nada da extensão é executado para descobrir o que ela é.** O projeto
  antigo rodava um `preparar.sh`; aqui o que o driver exige do ambiente
  (diretórios, variáveis, bibliotecas) é declarado.

Os manifestos de native messaging são os do fabricante: o Âncora troca só o
campo `path` ao publicar. O Firefox e a família Chromium usam formatos
diferentes (`allowed_extensions` e `allowed_origins`), e um manifesto no
formato da outra família é recusado na publicação: o navegador o ignoraria
sem avisar.

O assinador roda no ambiente de quem o chama: no sandbox do navegador, quando
ele é Flatpak, ou no do Âncora, quando ele é nativo. Nos dois, cada nome em
`pkcs11` vira um link para o cliente do p11-kit do runtime, que entrega os
tokens de todos os drivers. É o nome que a extensão do navegador manda o
assinador carregar (no Web PKI e no WebSigner, `libaetpkss.so.3`, a opção
"Dispositivos SafeSign AET"), e um só basta: dois nomes marcados na extensão
fariam cada token aparecer duas vezes. O executável recebe uma `HOME` própria,
onde guarda os dados dele, e precisa rodar nos runtimes freedesktop e GNOME.

A home do sandbox some a cada execução, e o serviço que carrega os drivers e a
janela que abre as ferramentas são instâncias diferentes. Um driver que guarda
dados do usuário em `$HOME` precisa de uma `HOME` em
`~/.local/share/ancora/casa/<Nome>`, a pasta de dados do Âncora, que é a do
host: no `ambiente` do `[[pkcs11]]` e no da `[[ferramenta]]`, apontando para a
mesma. O SerproID é o exemplo; um programa Java tira a home do passwd, e o
lançador passa a mesma pasta em `-Duser.home`.

Um assinador cujo fabricante traz um manifesto para o Chrome e outro para o
Edge, com ids diferentes (o Lacuna), junta as duas listas `allowed_origins` no
`apply_extra`: o Âncora publica um manifesto só para a família Chromium.

O atalho de menu de uma `[[ferramenta]]` é gerado pelo Âncora a partir de
`nome` e `icone`, porque o Flatpak não exporta o `.desktop` de uma extensão
instalada depois do aplicativo.

## Nome e descrição na busca

A janela mostra o nome e a descrição do appstream do remote, e sem ele só o
último pedaço do id (`SafeSign`). Para aparecer direito:

- a extensão instala um metainfo `addon` em
  `${FLATPAK_DEST}/share/metainfo/<id>.metainfo.xml`, sem
  `appstream-compose: false` no manifesto:

  ```xml
  <component type="addon">
    <id>dev.lukakuuhaku.Ancora.Driver.SafeSign</id>
    <extends>dev.lukakuuhaku.Ancora</extends>
    <name>SafeSign IC</name>
    <summary>Tokens GD Burti e outros SafeSign</summary>
    <metadata_license>CC0-1.0</metadata_license>
    <project_license>LicenseRef-proprietary</project_license>
  </component>
  ```

- quem publica o remote roda `flatpak build-update-repo` depois de exportar:
  é ele que gera o ramo `appstream` que os clientes baixam.

## Uma extensão quebrada continua visível

Descritor ilegível, inválido ou de contrato mais novo não some da lista: a
interface mostra a extensão com o motivo, para que a pessoa possa atualizá-la
ou removê-la. Pelo terminal:

```bash
flatpak run --command=ancora dev.lukakuuhaku.Ancora extensoes
```

lista cada extensão montada pelo ref, e sai com erro se alguma estiver
quebrada.
