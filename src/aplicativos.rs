//! Os aplicativos Flatpak que usam os tokens, e o que cada um precisa.
//!
//! Dois passos, os mesmos para todos, como no Tessera, de onde vêm as
//! receitas (testadas com token real em 29/10/2026):
//!
//! 1. **permissão**: acesso ao socket do `p11-kit-server.socket` do usuário,
//!    que serve os módulos do host, os do Âncora entre eles;
//! 2. **banco**: deixar o NSS do aplicativo chegar aos tokens exatamente uma
//!    vez. No Firefox, registrar o cliente do p11-kit em cada perfil. No
//!    Chromium e derivados não há o que registrar (o módulo de confiança do
//!    runtime já chega ao servidor), só tirar um registro do p11-kit que
//!    duplicaria os tokens. No Papers e no Okular, um banco vazio onde o
//!    poppler procura primeiro, pelo mesmo motivo.

use crate::{host, nss, permissao};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receita {
    Firefox,
    Chromium,
    Poppler,
    /// Já vem com a permissão e o banco certos.
    Pronto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Aplicativo {
    pub id: &'static str,
    pub nome: &'static str,
    pub receita: Receita,
}

pub const CONHECIDOS: [Aplicativo; 8] = [
    Aplicativo {
        id: "org.mozilla.firefox",
        nome: "Firefox",
        receita: Receita::Firefox,
    },
    Aplicativo {
        id: "org.chromium.Chromium",
        nome: "Chromium",
        receita: Receita::Chromium,
    },
    Aplicativo {
        id: "com.google.Chrome",
        nome: "Google Chrome",
        receita: Receita::Chromium,
    },
    Aplicativo {
        id: "com.microsoft.Edge",
        nome: "Microsoft Edge",
        receita: Receita::Chromium,
    },
    Aplicativo {
        id: "com.brave.Browser",
        nome: "Brave",
        receita: Receita::Chromium,
    },
    Aplicativo {
        id: "org.gnome.Papers",
        nome: "Papers",
        receita: Receita::Poppler,
    },
    Aplicativo {
        id: "org.kde.okular",
        nome: "Okular",
        receita: Receita::Poppler,
    },
    // O pacote do FlatPark já tem a permissão e carrega o cliente do p11-kit.
    Aplicativo {
        id: "br.jus.cnj.PJeOfficePro",
        nome: "PJeOffice Pro",
        receita: Receita::Pronto,
    },
];

pub fn por_id(id: &str) -> Option<Aplicativo> {
    CONHECIDOS.iter().copied().find(|a| a.id == id)
}

/// O banco que o poppler encontra primeiro: o perfil `*default*` mais
/// recente em `$XDG_CONFIG_HOME/mozilla/firefox`.
const BANCO_POPPLER: &str = "config/mozilla/firefox/ancora.default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banco {
    NaoSeAplica,
    Pronto,
    /// O que falta, em poucas palavras.
    Falta(String),
    /// O aplicativo está aberto e o banco não pode ser mexido agora.
    EmUso,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Situacao {
    pub permissao: bool,
    pub banco: Banco,
}

impl Situacao {
    pub fn pronto(&self) -> bool {
        self.permissao && matches!(self.banco, Banco::Pronto | Banco::NaoSeAplica)
    }
}

/// Onde moram os perfis do Firefox em Flatpak: em `config/` a partir do
/// Firefox 147, em `.mozilla/` antes.
fn raizes_firefox(home: &Path) -> [PathBuf; 2] {
    let base = host::var_app(home, "org.mozilla.firefox");
    [
        base.join("config/mozilla/firefox"),
        base.join(".mozilla/firefox"),
    ]
}

/// Os bancos que um navegador da família Chromium pode estar usando: desde o
/// Chromium 146, `$XDG_DATA_HOME/pki/nssdb`, salvo se o antigo existir.
fn bancos_chromium(home: &Path, id: &str) -> Vec<PathBuf> {
    let base = host::var_app(home, id);
    let mut candidatos = vec![base.join("data/pki/nssdb"), base.join(".pki/nssdb")];
    if id == "org.chromium.Chromium" {
        // O pacote do Chromium pode usar a home, e então o banco é o real.
        candidatos.push(home.join(".pki/nssdb"));
    }
    candidatos
        .into_iter()
        .filter(|b| b.join("cert9.db").is_file())
        .collect()
}

fn override_de(home: &Path, id: &str) -> PathBuf {
    host::overrides(home).join(id)
}

pub fn situacao(app: &Aplicativo, home: &Path) -> Situacao {
    let override_atual = fs::read_to_string(override_de(home, app.id)).unwrap_or_default();
    let permissao = app.receita == Receita::Pronto || permissao::tem(&override_atual);
    let banco = match app.receita {
        Receita::Pronto => Banco::NaoSeAplica,
        Receita::Firefox => {
            let perfis: Vec<PathBuf> = raizes_firefox(home)
                .iter()
                .flat_map(|r| nss::bancos(r))
                .collect();
            if perfis.is_empty() {
                Banco::Falta("abra o Firefox uma vez para criar o perfil".into())
            } else if perfis.iter().all(|p| nss::tem_modulo(&nss::ler(p))) {
                Banco::Pronto
            } else if perfis.iter().any(|p| nss::em_uso(p)) {
                Banco::EmUso
            } else {
                Banco::Falta("registrar o módulo nos perfis".into())
            }
        }
        Receita::Chromium => {
            if bancos_chromium(home, app.id)
                .iter()
                .any(|b| nss::tem_p11_kit(&nss::ler(b)))
            {
                Banco::Falta("tirar o módulo que duplica os tokens".into())
            } else {
                Banco::Pronto
            }
        }
        Receita::Poppler => {
            let banco = host::var_app(home, app.id).join(BANCO_POPPLER);
            if !banco.join("cert9.db").is_file() {
                Banco::Falta("criar o banco de certificados".into())
            } else if nss::tem_p11_kit(&nss::ler(&banco)) {
                Banco::Falta("tirar o módulo que duplica os tokens".into())
            } else {
                Banco::Pronto
            }
        }
    };
    Situacao { permissao, banco }
}

/// O registro das permissões que o Âncora concedeu, para desfazer só elas:
/// uma permissão que já estava lá é de outra pessoa ou programa.
fn concessoes(dados: &Path) -> PathBuf {
    dados.join("concessoes")
}

fn ler_concessoes(dados: &Path) -> BTreeSet<String> {
    fs::read_to_string(concessoes(dados))
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .filter(|l| !l.is_empty())
        .collect()
}

fn gravar_concessoes(dados: &Path, ids: &BTreeSet<String>) -> Result<(), String> {
    fs::create_dir_all(dados).map_err(|e| e.to_string())?;
    let texto: String = ids.iter().map(|i| format!("{i}\n")).collect();
    fs::write(concessoes(dados), texto).map_err(|e| e.to_string())
}

fn gravar(caminho: &Path, texto: &str) -> Result<(), String> {
    if let Some(pai) = caminho.parent() {
        fs::create_dir_all(pai).map_err(|e| format!("{}: {e}", pai.display()))?;
    }
    fs::write(caminho, texto).map_err(|e| format!("{}: {e}", caminho.display()))
}

/// Faz os dois passos. A permissão só vale depois que o aplicativo for
/// fechado e aberto de novo.
pub fn configurar(app: &Aplicativo, home: &Path, dados: &Path) -> Result<(), String> {
    if app.receita == Receita::Pronto {
        return Ok(());
    }

    let arquivo = override_de(home, app.id);
    let atual = fs::read_to_string(&arquivo).unwrap_or_default();
    if !permissao::tem(&atual) {
        gravar(&arquivo, &permissao::conceder(&atual)?)?;
        let mut ids = ler_concessoes(dados);
        ids.insert(app.id.to_owned());
        gravar_concessoes(dados, &ids)?;
    }

    match app.receita {
        Receita::Firefox => {
            let perfis: Vec<PathBuf> = raizes_firefox(home)
                .iter()
                .flat_map(|r| nss::bancos(r))
                .collect();
            if perfis.iter().any(|p| nss::em_uso(p)) {
                return Err("feche o Firefox antes".into());
            }
            for p in perfis {
                nss::editar(&p, |t| nss::com_modulo(t, nss::CLIENTE_NO_SANDBOX))
                    .map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
        Receita::Chromium => {
            for b in bancos_chromium(home, app.id) {
                nss::editar(&b, nss::sem_p11_kit).map_err(|e| format!("{}: {e}", b.display()))?;
            }
        }
        Receita::Poppler => {
            let banco = host::var_app(home, app.id).join(BANCO_POPPLER);
            if !banco.join("cert9.db").is_file() {
                nss::criar_banco(&banco)?;
            }
            nss::editar(&banco, nss::sem_p11_kit)
                .map_err(|e| format!("{}: {e}", banco.display()))?;
        }
        Receita::Pronto => {}
    }
    Ok(())
}

/// Desfaz o que o Âncora fez no aplicativo, e só isso. Os registros
/// duplicados tirados de um Chromium não voltam: eram o defeito.
pub fn desfazer(app: &Aplicativo, home: &Path, dados: &Path) -> Result<(), String> {
    let mut ids = ler_concessoes(dados);
    if ids.contains(app.id) {
        let arquivo = override_de(home, app.id);
        let atual = fs::read_to_string(&arquivo).unwrap_or_default();
        match permissao::revogar(&atual)? {
            Some(novo) => gravar(&arquivo, &novo)?,
            None => match fs::remove_file(&arquivo) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(format!("{}: {e}", arquivo.display()));
                }
                _ => {}
            },
        }
        ids.remove(app.id);
        gravar_concessoes(dados, &ids)?;
    }

    match app.receita {
        Receita::Firefox => {
            let perfis: Vec<PathBuf> = raizes_firefox(home)
                .iter()
                .flat_map(|r| nss::bancos(r))
                .collect();
            if perfis.iter().any(|p| nss::em_uso(p)) {
                return Err("feche o Firefox antes".into());
            }
            for p in perfis {
                nss::editar(&p, nss::sem_modulo).map_err(|e| format!("{}: {e}", p.display()))?;
            }
        }
        Receita::Poppler => {
            let banco = host::var_app(home, app.id).join(BANCO_POPPLER);
            match fs::remove_dir_all(&banco) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    return Err(format!("{}: {e}", banco.display()));
                }
                _ => {}
            }
        }
        Receita::Chromium | Receita::Pronto => {}
    }
    Ok(())
}

/// Os aplicativos Flatpak instalados no host, de qualquer instalação.
pub fn instalados() -> Result<BTreeSet<String>, String> {
    let saida = Command::new("flatpak-spawn")
        .args([
            "--host",
            "flatpak",
            "list",
            "--app",
            "--columns=application",
        ])
        .output()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    if !saida.status.success() {
        return Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned());
    }
    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        .map(|l| l.trim().to_owned())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Liga o `p11-kit-server.socket` do usuário no host. Sem ele, nenhum
/// aplicativo Flatpak chega aos tokens.
pub fn ligar_servidor_do_sistema() -> Result<(), String> {
    let saida = Command::new("flatpak-spawn")
        .args([
            "--host",
            "systemctl",
            "--user",
            "enable",
            "--now",
            "p11-kit-server.socket",
        ])
        .output()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    if saida.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned())
    }
}

/// O socket do servidor do sistema, visto de dentro deste sandbox.
pub fn socket_do_sistema() -> PathBuf {
    gtk::glib::user_runtime_dir().join("p11-kit/pkcs11")
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Home(PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn home(nome: &str) -> Home {
        let p = std::env::temp_dir().join(format!("ancora-apps-{}-{nome}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Home(p)
    }
    fn app(id: &str) -> Aplicativo {
        por_id(id).unwrap()
    }

    #[test]
    fn os_ids_do_catalogo_sao_unicos() {
        let ids: BTreeSet<_> = CONHECIDOS.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), CONHECIDOS.len());
    }

    #[test]
    fn pjeoffice_do_flatpark_ja_esta_pronto() {
        let h = home("pje");
        let s = situacao(&app("br.jus.cnj.PJeOfficePro"), &h.0);
        assert!(s.pronto());
        configurar(&app("br.jus.cnj.PJeOfficePro"), &h.0, &h.0.join("dados")).unwrap();
        assert!(
            !host::overrides(&h.0).exists(),
            "não deveria criar override"
        );
    }

    #[test]
    fn papers_ganha_permissao_e_banco_vazio_e_desfazer_tira_os_dois() {
        let h = home("papers");
        let dados = h.0.join("dados");
        let papers = app("org.gnome.Papers");
        assert_eq!(
            situacao(&papers, &h.0),
            Situacao {
                permissao: false,
                banco: Banco::Falta("criar o banco de certificados".into())
            }
        );

        configurar(&papers, &h.0, &dados).unwrap();
        assert!(situacao(&papers, &h.0).pronto());

        desfazer(&papers, &h.0, &dados).unwrap();
        assert!(!host::overrides(&h.0).join("org.gnome.Papers").exists());
        assert!(
            !host::var_app(&h.0, "org.gnome.Papers")
                .join(BANCO_POPPLER)
                .exists()
        );
    }

    #[test]
    fn permissao_que_ja_existia_nao_e_revogada() {
        let h = home("ja-tinha");
        let dados = h.0.join("dados");
        let arquivo = host::overrides(&h.0).join("org.gnome.Papers");
        gravar(&arquivo, "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11;\n").unwrap();

        configurar(&app("org.gnome.Papers"), &h.0, &dados).unwrap();
        desfazer(&app("org.gnome.Papers"), &h.0, &dados).unwrap();
        assert!(permissao::tem(&fs::read_to_string(&arquivo).unwrap()));
    }

    #[test]
    fn firefox_registra_em_todos_os_perfis_e_recusa_perfil_aberto_so_por_lock() {
        let h = home("firefox");
        let dados = h.0.join("dados");
        let raiz = host::var_app(&h.0, "org.mozilla.firefox").join("config/mozilla/firefox");
        for p in ["a.default", "b.default-release"] {
            fs::create_dir_all(raiz.join(p)).unwrap();
            fs::write(raiz.join(p).join("cert9.db"), "").unwrap();
            fs::write(
                raiz.join(p).join("pkcs11.txt"),
                "library=\nname=NSS Internal PKCS #11 Module\n\n",
            )
            .unwrap();
        }
        let firefox = app("org.mozilla.firefox");
        assert_eq!(
            situacao(&firefox, &h.0).banco,
            Banco::Falta("registrar o módulo nos perfis".into())
        );

        configurar(&firefox, &h.0, &dados).unwrap();
        assert!(situacao(&firefox, &h.0).pronto());
        for p in ["a.default", "b.default-release"] {
            assert!(nss::tem_modulo(&nss::ler(&raiz.join(p))));
        }

        desfazer(&firefox, &h.0, &dados).unwrap();
        for p in ["a.default", "b.default-release"] {
            let texto = nss::ler(&raiz.join(p));
            assert!(!nss::tem_modulo(&texto));
            assert!(texto.contains("NSS Internal"));
        }
    }

    #[test]
    fn firefox_sem_perfil_pede_para_abrir_uma_vez() {
        let h = home("firefox-novo");
        assert_eq!(
            situacao(&app("org.mozilla.firefox"), &h.0).banco,
            Banco::Falta("abra o Firefox uma vez para criar o perfil".into())
        );
    }

    #[test]
    fn chromium_com_registro_do_p11_kit_e_limpo() {
        let h = home("brave");
        let banco = host::var_app(&h.0, "com.brave.Browser").join("data/pki/nssdb");
        fs::create_dir_all(&banco).unwrap();
        fs::write(banco.join("cert9.db"), "").unwrap();
        fs::write(
            banco.join("pkcs11.txt"),
            "library=\nname=NSS Internal PKCS #11 Module\n\nlibrary=/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so\nname=Tessera (p11-kit)\n\n",
        )
        .unwrap();
        let brave = app("com.brave.Browser");
        assert!(!situacao(&brave, &h.0).pronto());
        configurar(&brave, &h.0, &h.0.join("dados")).unwrap();
        assert!(situacao(&brave, &h.0).pronto());
        assert!(nss::ler(&banco).contains("NSS Internal"));
    }
}
