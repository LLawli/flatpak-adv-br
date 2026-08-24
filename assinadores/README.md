# Assinadores como extensões

Assinador é o programa que o navegador executa para falar com o token: o site
pede uma assinatura, a extensão do navegador conversa com ele por
stdin/stdout, e ele usa a chave privada. São três, de fabricantes diferentes,
e cada um serve a um conjunto de sistemas:

| extensão | para que serve | tamanho |
|---|---|---|
| `--with-webpki` | Lacuna Web PKI | ~142 MB |
| `--with-websigner` | Softplan WebSigner, dos sistemas SAJ | ~140 MB |
| `--with-certisign` | Certisign WebSigner, do portal da OAB | ~2 MB |

Nenhum vem no pacote base, por dois motivos que se somam: eles não podem ser
redistribuídos — cada manifesto baixa da URL do próprio fabricante, na sua
máquina — e quem usa só um não deve baixar os três.

```sh
./instalar.sh --with-webpki        # só o que você usa
./instalar.sh --with-assinadores   # os três
```

Instalar depois é rodar de novo com a outra opção: o que já está pronto não é
refeito.

## A convenção que uma extensão de assinador segue

O prefixo de build é `/app/lib/assinadores/<Nome>`, e dentro dele:

| caminho | o que é |
|---|---|
| `bin/<nome>` | o executável. O nome importa: `bin/webpki` é o que o comando `adv-br-webpki` procura, e é assim que o pacote base sabe lançar um assinador que ele não conhece. |
| `native-messaging/<host-name>.<familia>.json` | os manifestos que o `.deb` do fabricante instalou, um por família de navegador (`firefox` e `chromium`). |
| `lib/` | bibliotecas de apoio. Entra no `LD_LIBRARY_PATH` antes de qualquer carga. |
| `preparar.sh` | opcional, executável. Roda antes do assinador subir. |

Os manifestos vêm do fabricante e **não** são reescritos aqui: quem responde
por "quais extensões de navegador podem falar com este assinador" é ele. O
`./host/publicar.sh` copia o arquivo trocando um campo só, o `path`.

As duas famílias de navegador exigem campos diferentes — `allowed_extensions`
no Firefox, `allowed_origins` na família Chromium — e um navegador que receba o
formato do outro **ignora o arquivo em silêncio**, deixando a extensão dizer
que o assinador não está instalado. Por isso são dois arquivos, e por isso o
`./diagnostico.sh` confere o formato de cada um.

## O que se sabe de cada um

- **Lacuna e Softplan** são .NET com Avalonia, que só tem backend X11. Sob
  Wayland eles morrem com `XOpenDisplay failed` antes de ler a primeira
  mensagem — daí o `--socket=x11` de verdade no pacote base, e não
  `fallback-x11`.
- **Assinar exige licença da aplicação**, que quem fornece é o site. Um teste
  de linha de comando consegue listar certificados e nunca vai assinar: a
  resposta é `CheckLicenseAsync`, e não é falha do empacotamento.
- **Certisign** não responde a `getVersion` — o protocolo dele conhece
  `getInfo`, `listCertificates`, `listTokens` e `sign`. Verificado que ele se
  comporta igual fora do Flatpak, no host e num contêiner Debian.
- **Certisign** abre os `.glade` da interface por caminho absoluto em `/opt`,
  o que o `preparar.sh` dele acomoda: a raiz do sandbox é um tmpfs gravável.
- **Softplan** é o único módulo que baixa durante a construção, porque o
  servidor dele serve uma cadeia TLS incompleta. Ver `packaging/ca/README.md`.
