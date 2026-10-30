//! O manifesto Flatpak constrói sem rede: cada crate do Cargo.lock precisa
//! estar no flatpak/cargo-sources.json. Um crate faltando só aparece no
//! flatpak-builder, minutos depois, como "failed to download". Este teste
//! acusa antes.

use std::fs;
use std::path::Path;

/// Os crates do crates.io no Cargo.lock, como `nome-versão`.
fn crates_do_lock(lock: &str) -> Vec<String> {
    lock.split("[[package]]")
        .filter(|bloco| bloco.contains("source = \"registry+"))
        .map(|bloco| {
            let campo = |chave: &str| {
                bloco
                    .lines()
                    .find_map(|linha| linha.strip_prefix(chave))
                    .and_then(|resto| resto.strip_prefix(" = \""))
                    .and_then(|resto| resto.strip_suffix('"'))
                    .unwrap_or_else(|| panic!("pacote sem {chave} no Cargo.lock:{bloco}"))
                    .to_owned()
            };
            format!("{}-{}", campo("name"), campo("version"))
        })
        .collect()
}

#[test]
fn todo_crate_do_lock_esta_no_cargo_sources() {
    let raiz = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lock = fs::read_to_string(raiz.join("Cargo.lock")).expect("Cargo.lock");
    let fontes = fs::read_to_string(raiz.join("flatpak/cargo-sources.json"))
        .expect("flatpak/cargo-sources.json: rode scripts/cargo-sources.py");

    let crates = crates_do_lock(&lock);
    // Um lock lido errado daria zero crates, e zero faltando passaria.
    assert!(
        crates.len() > 50,
        "só {} crates lidos do Cargo.lock",
        crates.len()
    );

    let faltando: Vec<_> = crates
        .iter()
        .filter(|c| !fontes.contains(&format!("/{c}.crate\"")))
        .collect();
    assert!(
        faltando.is_empty(),
        "fora do cargo-sources.json (rode scripts/cargo-sources.py): {faltando:?}"
    );
}
