# Desenvolvimento

## Dependências

- Rust estável (1.92 ou mais novo).
- GTK 4.18 e libadwaita 1.7, ou mais novos, com os cabeçalhos
  (`gtk4-devel libadwaita-devel` no Fedora).
- `desktop-file-utils` e `appstream`, para validar os arquivos de dados.
- `cargo-audit`.
- Para o Flatpak: `org.flatpak.Builder`, `org.gnome.Sdk//51` e
  `org.freedesktop.Sdk.Extension.rust-stable//26.08`.

## Gates

```bash
scripts/ci.sh
```

Formatação, clippy com avisos como erro, testes, `cargo audit` e validação do
`.desktop` e do metainfo. A CI roda o mesmo script, num contêiner Fedora 44,
porque o Ubuntu dos runners traz GTK e libadwaita mais antigos que os do
runtime.

## Dependências Rust e o Flatpak

O `flatpak-builder` constrói sem rede. Cada crate do `Cargo.lock` precisa estar
em `flatpak/cargo-sources.json`, e depois de mexer nas dependências é preciso
regenerar:

```bash
scripts/cargo-sources.py
```

O teste `tests/flatpak.rs` falha enquanto os dois estiverem fora de sincronia.
O script cobre só crates do crates.io; uma dependência git é recusada.

## Construir o Flatpak

```bash
flatpak run org.flatpak.Builder --user --install --force-clean build-dir flatpak/dev.lukakuuhaku.Ancora.yml
flatpak run dev.lukakuuhaku.Ancora
```

## Extensões

Cada extensão é um repositório próprio em `extensoes/<Família>.<Nome>`, fora
do controle de versão do Âncora, com `scripts/ci.sh` e `scripts/construir.sh`.
O contrato está em [extensoes.md](extensoes.md), e o exemplo é o
`extensoes/Driver.SafeSign`.

### Testar sem o token

Um `pcscd` com leitoras e cartões virtuais, num contêiner, com o Âncora
apontado para ele no lugar do pcscd do host. Não substitui o token de verdade:
os drivers proprietários não reconhecem o cartão emulado. Serve para ver o
driver enxergar as leitoras, aguentar um cartão que não é dele e sobreviver a
cartão entrando e saindo, e para medir o tempo disso.

```bash
X=/mnt/dados/caches/ancora-exp/pcsc-falso; mkdir -p "$X/run"
docker build -t ancora-pcsc-falso tests/pcsc-falso
docker run -d --rm --name ancora-pcsc-falso -v "$X/run:/run/pcscd" ancora-pcsc-falso
docker exec -d ancora-pcsc-falso vicc -t iso7816   # insere o cartão
docker exec ancora-pcsc-falso pkill -f '[v]icc'    # retira

# --nosocket=pcsc: com o socket, o Flatpak sobrescreve o PCSCLITE_CSOCK_NAME.
flatpak run --nosocket=pcsc --filesystem="$X/run" \
    --env=PCSCLITE_CSOCK_NAME="$X/run/pcscd.comm" \
    --command=ancora dev.lukakuuhaku.Ancora servir
```

## Publicar

```bash
cp config/publicar.env.example config/publicar.env   # e preencha
bin/publicar            # constrói o Âncora e as extensões e assina
bin/publicar --enviar   # e manda para o servidor
```

O repositório local é o do AdvBr (`~/.local/share/adv-br-repo`), porque o
`end-of-life-rebase` só leva quem usa o AdvBr ao Âncora se os dois estiverem
no mesmo remote. A assinatura é feita fora do sandbox do builder, com a chave
da YubiKey (um toque por ref, e um pelo summary), e o envio por `rsync` pede
outro toque. O `--enviar` se recusa a mandar um repositório sem o AdvBr: com
`--delete`, ele o apagaria do servidor. A ordem da assinatura e as armadilhas
estão comentadas no próprio `bin/publicar`.

Para testar sem a chave real, uma chave descartável num `GNUPGHOME` próprio:
`ANCORA_REPO`, `ANCORA_TRABALHO`, `ANCORA_CHAVE_GPG` e `ANCORA_CHAVE_PUBLICA`
apontando para ela e para um repositório temporário.

## Ponta a ponta

`tests/e2e/` tem testes que rodam no sandbox de verdade, com extensões de
teste construídas na hora. Não fazem parte do `scripts/ci.sh`; veja
`tests/e2e/README.md`.
