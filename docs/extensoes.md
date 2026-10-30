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
extensão.

## O descritor: `ancora.toml`

Fica na raiz da extensão (`${FLATPAK_DEST}/ancora.toml`). Diretório montado
sem ele não é extensão do Âncora e é ignorado.

```toml
contrato = 1                      # obrigatório
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

O atalho de menu de uma `[[ferramenta]]` é gerado pelo Âncora a partir de
`nome` e `icone`, porque o Flatpak não exporta o `.desktop` de uma extensão
instalada depois do aplicativo.

## Uma extensão quebrada continua visível

Descritor ilegível, inválido ou de contrato mais novo não some da lista: a
interface mostra a extensão com o motivo, para que a pessoa possa atualizá-la
ou removê-la. Pelo terminal:

```bash
flatpak run --command=ancora dev.lukakuuhaku.Ancora extensoes
```

lista cada extensão montada pelo ref, e sai com erro se alguma estiver
quebrada.
