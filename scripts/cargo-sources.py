#!/usr/bin/env python3
"""Regenera flatpak/cargo-sources.json a partir do Cargo.lock.

O flatpak-builder constrói sem rede, então cada crate do Cargo.lock precisa
estar no manifesto como fonte, com o sha256. Rode depois de qualquer mudança no
Cargo.lock; o teste tests/flatpak.rs falha enquanto os dois estiverem fora de
sincronia.

Faz o mesmo que o flatpak-cargo-generator do flatpak-builder-tools para crates
do crates.io, que são os únicos que este projeto usa: o sha256 de cada um já
está no Cargo.lock, então nada é baixado. Dependência git é recusada em vez de
ignorada.
"""
import json
import pathlib
import sys
import tomllib

RAIZ = pathlib.Path(__file__).resolve().parent.parent
REGISTRO = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"

CONFIG = f"""\
[source.vendored-sources]
directory = "{VENDOR}"

[source.crates-io]
replace-with = "vendored-sources"
"""


def fontes(lock):
    saida = []
    for pacote in lock["package"]:
        origem = pacote.get("source")
        if origem is None:
            continue  # o próprio projeto
        if origem != REGISTRO:
            sys.exit(f"origem não suportada para {pacote['name']}: {origem}")
        nome, versao, soma = pacote["name"], pacote["version"], pacote["checksum"]
        destino = f"{VENDOR}/{nome}-{versao}"
        saida.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{nome}/{nome}-{versao}.crate",
            "sha256": soma,
            "dest": destino,
        })
        saida.append({
            "type": "inline",
            "contents": json.dumps({"package": soma, "files": {}}),
            "dest": destino,
            "dest-filename": ".cargo-checksum.json",
        })
    saida.append({
        "type": "inline",
        "contents": CONFIG,
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    return saida


def main():
    lock = tomllib.loads((RAIZ / "Cargo.lock").read_text())
    destino = RAIZ / "flatpak" / "cargo-sources.json"
    destino.write_text(json.dumps(fontes(lock), indent=4) + "\n")


if __name__ == "__main__":
    main()
