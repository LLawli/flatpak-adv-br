#!/usr/bin/env bash
# Todos os gates do projeto, na ordem do mais barato para o mais caro. A CI
# roda este mesmo script: verde aqui é verde lá.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
cargo audit

desktop-file-validate data/dev.lukakuuhaku.Ancora.desktop
appstreamcli validate --no-net --explain data/dev.lukakuuhaku.Ancora.metainfo.xml
