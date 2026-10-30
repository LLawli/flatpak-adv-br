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

## Ponta a ponta

`tests/e2e/` tem testes que rodam no sandbox de verdade, com extensões de
teste construídas na hora. Não fazem parte do `scripts/ci.sh`; veja
`tests/e2e/README.md`.
