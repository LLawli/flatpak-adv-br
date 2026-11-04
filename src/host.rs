//! Os caminhos do host, vistos de dentro do sandbox.

use std::path::{Path, PathBuf};
use std::process::Command;

/// O manifesto é a fonte das pastas dos outros aplicativos que o sandbox vê.
const MANIFESTO: &str = include_str!("../flatpak/dev.lukakuuhaku.Ancora.yml");

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
pub fn var_app(home: &Path, id: &str) -> PathBuf {
    home.join(".var/app").join(id)
}

/// Os overrides de permissão que `flatpak override --user` escreve.
pub fn overrides(home: &Path) -> PathBuf {
    home.join(".local/share/flatpak/overrides")
}

/// As pastas da home que o manifesto dá ao sandbox, sem as opções.
fn pastas_concedidas(manifesto: &str, home: &Path) -> Vec<PathBuf> {
    manifesto
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- --filesystem=~/"))
        .map(|p| home.join(p.split(':').next().unwrap_or(p)))
        .collect()
}

/// Se uma pasta concedida apareceu no host depois que este sandbox começou.
/// O Flatpak só monta a que já existia, e sem reiniciar o Âncora não enxerga
/// o perfil de um navegador aberto pela primeira vez depois dele.
pub fn surgiu_pasta_depois(home: &Path) -> bool {
    if std::env::var_os(VARIAVEL).is_some() {
        return false;
    }
    let ausentes: Vec<PathBuf> = pastas_concedidas(MANIFESTO, home)
        .into_iter()
        .filter(|p| !p.exists())
        .collect();
    if ausentes.is_empty() {
        return false;
    }
    Command::new("flatpak-spawn")
        .args(["--host", "sh", "-c"])
        .arg(r#"for p do [ -e "$p" ] && exit 0; done; exit 1"#)
        .arg("sh")
        .args(&ausentes)
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn as_pastas_vem_do_manifesto_sem_as_opcoes() {
        let manifesto = "finish-args:\n  - --share=ipc\n  # - --filesystem=~/.comentario\n  \
            - --filesystem=xdg-config/pkcs11:create\n  - --filesystem=~/.mozilla\n  \
            - --filesystem=~/.var/app/org.kde.okular/config/mozilla:create\n";
        assert_eq!(
            pastas_concedidas(manifesto, Path::new("/h")),
            [
                PathBuf::from("/h/.mozilla"),
                PathBuf::from("/h/.var/app/org.kde.okular/config/mozilla"),
            ]
        );
    }

    #[test]
    fn o_manifesto_real_da_as_pastas_dos_navegadores() {
        let pastas = pastas_concedidas(MANIFESTO, Path::new("/h"));
        assert!(pastas.contains(&PathBuf::from("/h/.var/app/org.mozilla.firefox")));
        assert!(pastas.contains(&PathBuf::from("/h/.config/mozilla")));
        assert!(pastas.len() >= 20, "{}", pastas.len());
    }
}
