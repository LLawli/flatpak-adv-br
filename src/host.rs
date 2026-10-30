//! Os caminhos do host, vistos de dentro do sandbox.

use std::path::PathBuf;

/// Redireciona a home do host, para testes: tudo o que o Âncora escreve fora
/// do próprio sandbox (módulos, overrides, bancos NSS) passa a cair embaixo
/// dela.
pub const VARIAVEL: &str = "ANCORA_HOME_HOST";

/// A home do host. No sandbox, `$HOME` já é a home real.
pub fn home() -> PathBuf {
    match std::env::var_os(VARIAVEL) {
        Some(h) if !h.is_empty() => PathBuf::from(h),
        _ => gtk::glib::home_dir(),
    }
}

/// O diretório de dados de um aplicativo Flatpak.
pub fn var_app(home: &std::path::Path, id: &str) -> PathBuf {
    home.join(".var/app").join(id)
}

/// Os overrides de permissão que `flatpak override --user` escreve.
pub fn overrides(home: &std::path::Path) -> PathBuf {
    home.join(".local/share/flatpak/overrides")
}
