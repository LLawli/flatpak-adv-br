//! Os aplicativos Flatpak que usam os tokens, e o que cada um precisa.
//!
//! Dois passos, os mesmos para todos:
//!
//! 1. **permissão**: acesso ao socket do `p11-kit-server.socket` do usuário,
//!    que serve os módulos do host, os do Âncora entre eles;
//! 2. **banco**: deixar o NSS do aplicativo chegar aos tokens exatamente uma
//!    vez. Isso não é suposto, é medido (`verificacao`): o Âncora tenta não
//!    registrar nada, depois as formas de registro, e fica com a primeira em
//!    que o token-sentinela aparece uma vez dentro do sandbox do aplicativo.
//!    As opções e o padrão (usado quando não dá para medir) vêm das receitas
//!    que o Tessera testou com token real em 29/10/2026: no Firefox, o cliente
//!    do p11-kit em cada perfil; no Chromium e derivados, nada (o módulo de
//!    confiança do runtime já chega ao servidor); no Papers e no Okular, um
//!    banco vazio onde o poppler procura primeiro, e nada nele pelo mesmo
//!    motivo do Chromium: o poppler também carrega o módulo de confiança.

use crate::verificacao::{self, Ambiente, Medidor, Raizes, Registro, Resultado};
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

/// O Firefox traz a própria libnss3.
const NSS_DO_FIREFOX: &str = "/app/lib/firefox";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Banco {
    NaoSeAplica,
    /// Verificado: o token aparece uma vez, e o banco segue como ficou.
    Pronto,
    /// Configurado pela receita padrão, sem ter podido medir.
    NaoVerificado,
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
        self.permissao
            && matches!(
                self.banco,
                Banco::Pronto | Banco::NaoVerificado | Banco::NaoSeAplica
            )
    }
}

/// Onde moram os perfis do Firefox em Flatpak: em `config/` a partir do
/// Firefox 147, em `.mozilla/` antes.
fn perfis_firefox(home: &Path) -> Vec<PathBuf> {
    let base = host::var_app(home, "org.mozilla.firefox");
    [
        base.join("config/mozilla/firefox"),
        base.join(".mozilla/firefox"),
    ]
    .iter()
    .flat_map(|r| nss::bancos(r))
    .collect()
}

/// Os bancos que um navegador da família Chromium pode estar usando, do que
/// vale mais para o que vale menos: o antigo ganha se existir, e o pacote do
/// Chromium, que pode usar a home, usa o real.
fn bancos_chromium(home: &Path, id: &str) -> Vec<PathBuf> {
    let base = host::var_app(home, id);
    let mut candidatos = Vec::new();
    if id == "org.chromium.Chromium" {
        candidatos.push(home.join(".pki/nssdb"));
    }
    candidatos.push(base.join(".pki/nssdb"));
    candidatos.push(base.join("data/pki/nssdb"));
    candidatos
        .into_iter()
        .filter(|b| b.join("cert9.db").is_file())
        .collect()
}

/// Os bancos que o aplicativo de fato abre, e que a verificação mede.
fn bancos_medidos(app: &Aplicativo, home: &Path) -> Vec<PathBuf> {
    match app.receita {
        Receita::Firefox => perfis_firefox(home),
        Receita::Chromium => bancos_chromium(home, app.id).into_iter().take(1).collect(),
        Receita::Poppler => vec![host::var_app(home, app.id).join(BANCO_POPPLER)],
        Receita::Pronto => vec![],
    }
}

pub fn situacao(app: &Aplicativo, home: &Path, dados: &Path) -> Situacao {
    let permissao = app.receita == Receita::Pronto
        || permissao::tem_em(home, app.id, &permissao::Entrada::socket_do_sistema());
    if app.receita == Receita::Pronto {
        return Situacao {
            permissao,
            banco: Banco::NaoSeAplica,
        };
    }
    let bancos = bancos_medidos(app, home);
    let registros = verificacao::ler_registros(dados);
    let banco = if bancos.iter().all(|b| !b.join("cert9.db").is_file()) {
        Banco::Falta(match app.receita {
            Receita::Poppler => "criar o banco de certificados".into(),
            _ => format!("abra o {} uma vez para criar o perfil", app.nome),
        })
    } else if app.receita == Receita::Chromium
        && bancos_chromium(home, app.id)
            .iter()
            .any(|b| nss::tem_p11_kit(&nss::ler(b)))
    {
        Banco::Falta("tirar o módulo que duplica os tokens".into())
    } else {
        let estados: Vec<Option<&Registro>> = bancos
            .iter()
            .map(|b| {
                registros
                    .get(&(b.clone(), nss::NOME.to_owned()))
                    .filter(|r| r.vale_para(b, nss::NOME))
            })
            .collect();
        if estados.iter().any(Option::is_none) {
            if app.receita == Receita::Firefox && bancos.iter().any(|b| nss::em_uso(b)) {
                Banco::EmUso
            } else {
                Banco::Falta("verificar".into())
            }
        } else if estados.iter().all(|r| r.is_some_and(|r| r.verificado)) {
            Banco::Pronto
        } else {
            Banco::NaoVerificado
        }
    };
    Situacao { permissao, banco }
}

/// Faz os dois passos e devolve, por banco, o que a verificação decidiu. A
/// permissão só vale depois que o aplicativo for fechado e aberto de novo.
pub fn configurar(
    app: &Aplicativo,
    home: &Path,
    dados: &Path,
    medidor: &dyn Medidor,
) -> Result<Vec<(PathBuf, Resultado)>, String> {
    if app.receita == Receita::Pronto {
        return Ok(vec![]);
    }
    if app.receita != Receita::Poppler && bancos_medidos(app, home).is_empty() {
        return Err(format!("abra o {} uma vez para criar o perfil", app.nome));
    }

    permissao::conceder_a(
        home,
        dados,
        app.id,
        &permissao::Entrada::socket_do_sistema(),
    )?;

    let cliente = Some(nss::CLIENTE_NO_SANDBOX.to_owned());
    let (raizes, opcoes, padrao, nss_proprio) = match app.receita {
        Receita::Firefox => {
            if perfis_firefox(home).iter().any(|p| nss::em_uso(p)) {
                return Err("feche o Firefox antes".into());
            }
            let proxy = Some(nss::PROXY_NO_SANDBOX.to_owned());
            (
                Raizes::Automaticas,
                vec![None, cliente.clone(), proxy],
                cliente,
                Some(NSS_DO_FIREFOX),
            )
        }
        Receita::Chromium => {
            for b in bancos_chromium(home, app.id) {
                nss::editar(&b, nss::sem_p11_kit).map_err(|e| format!("{}: {e}", b.display()))?;
            }
            (Raizes::Chromium, vec![None, cliente], None, None)
        }
        Receita::Poppler => {
            let banco = host::var_app(home, app.id).join(BANCO_POPPLER);
            if !banco.join("cert9.db").is_file() {
                nss::criar_banco(&banco)?;
            }
            nss::editar(&banco, nss::sem_p11_kit)
                .map_err(|e| format!("{}: {e}", banco.display()))?;
            (Raizes::Chromium, vec![None, cliente], None, None)
        }
        Receita::Pronto => unreachable!(),
    };

    let ambiente = Ambiente::Flatpak {
        app: app.id,
        nss_proprio,
    };
    let mut saida = Vec::new();
    for banco in bancos_medidos(app, home) {
        if !banco.join("cert9.db").is_file() {
            continue;
        }
        let resultado = verificacao::garantir_um_no_banco(
            &banco,
            nss::NOME,
            medidor,
            ambiente,
            raizes,
            &opcoes,
            &padrao,
        )?;
        verificacao::gravar_registro(dados, &banco, nss::NOME, Some(Registro::de(&resultado)))?;
        saida.push((banco, resultado));
    }
    Ok(saida)
}

/// Desfaz o que o Âncora fez no aplicativo, e só isso. Os registros
/// duplicados tirados de um Chromium não voltam: eram o defeito.
pub fn desfazer(app: &Aplicativo, home: &Path, dados: &Path) -> Result<(), String> {
    permissao::revogar_de(
        home,
        dados,
        app.id,
        &permissao::Entrada::socket_do_sistema(),
    )?;

    match app.receita {
        Receita::Firefox => {
            let perfis = perfis_firefox(home);
            if perfis.iter().any(|p| nss::em_uso(p)) {
                return Err("feche o Firefox antes".into());
            }
            for p in perfis {
                nss::editar(&p, |t| nss::sem_modulo(t, nss::NOME))
                    .map_err(|e| format!("{}: {e}", p.display()))?;
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
        Receita::Chromium | Receita::Pronto => {
            for b in bancos_chromium(home, app.id) {
                nss::editar(&b, |t| nss::sem_modulo(t, nss::NOME))
                    .map_err(|e| format!("{}: {e}", b.display()))?;
            }
        }
    }
    // Os bancos medidos já foram calculados acima com o estado de antes; o
    // do poppler pode ter acabado de ser apagado.
    let mut esquecer = bancos_medidos(app, home);
    if app.receita == Receita::Chromium {
        esquecer = bancos_chromium(home, app.id);
    }
    for banco in esquecer {
        verificacao::gravar_registro(dados, &banco, nss::NOME, None)?;
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
    use crate::verificacao::Contagem;

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

    /// Um consumidor que só vê o token pelo bloco do Âncora (o Firefox).
    fn so_com_bloco(_: Ambiente, banco: &Path, _: Raizes) -> Result<Contagem, String> {
        let vezes = usize::from(nss::tem_modulo(&nss::ler(banco), nss::NOME));
        Ok(Contagem {
            vezes,
            modulos: vec![],
        })
    }

    /// Um consumidor que já vê o token pelo módulo de confiança, e duplica
    /// com o bloco (o Chromium, o Papers).
    fn ja_ve(_: Ambiente, banco: &Path, _: Raizes) -> Result<Contagem, String> {
        let vezes = 1 + usize::from(nss::tem_modulo(&nss::ler(banco), nss::NOME));
        Ok(Contagem {
            vezes,
            modulos: vec![],
        })
    }

    fn banco(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("cert9.db"), "").unwrap();
        fs::write(
            dir.join("pkcs11.txt"),
            "library=\nname=NSS Internal PKCS #11 Module\n\n",
        )
        .unwrap();
    }

    #[test]
    fn o_poppler_e_medido_como_o_chromium() {
        // O poppler carrega ele mesmo o "Root Certs". Medido como o Firefox,
        // o token parecia faltar, o Âncora registrava o cliente e o Okular
        // mostrava cada certificado e pedia o PIN duas vezes.
        let h = home("poppler-raizes");
        let vistas = std::cell::RefCell::new(vec![]);
        let medidor = |_: Ambiente, banco: &Path, raizes: Raizes| {
            vistas.borrow_mut().push(raizes);
            ja_ve(Ambiente::Host, banco, raizes)
        };
        configurar(&app("org.kde.okular"), &h.0, &h.0.join("dados"), &medidor).unwrap();
        assert_eq!(*vistas.borrow(), [Raizes::Chromium]);
    }

    #[test]
    fn sem_perfil_nao_finge_que_configurou() {
        let h = home("sem-perfil");
        for id in ["org.mozilla.firefox", "org.chromium.Chromium"] {
            let e = configurar(&app(id), &h.0, &h.0.join("dados"), &ja_ve).unwrap_err();
            assert!(e.contains("uma vez"), "{e}");
        }
    }

    #[test]
    fn os_ids_do_catalogo_sao_unicos() {
        let ids: BTreeSet<_> = CONHECIDOS.iter().map(|a| a.id).collect();
        assert_eq!(ids.len(), CONHECIDOS.len());
    }

    #[test]
    fn pjeoffice_do_flatpark_ja_esta_pronto() {
        let h = home("pje");
        let dados = h.0.join("dados");
        assert!(situacao(&app("br.jus.cnj.PJeOfficePro"), &h.0, &dados).pronto());
        configurar(&app("br.jus.cnj.PJeOfficePro"), &h.0, &dados, &so_com_bloco).unwrap();
        assert!(
            !host::overrides(&h.0).exists(),
            "não deveria criar override"
        );
    }

    #[test]
    fn papers_ganha_permissao_e_banco_vazio_sem_bloco_e_desfazer_tira_tudo() {
        let h = home("papers");
        let dados = h.0.join("dados");
        let papers = app("org.gnome.Papers");
        assert_eq!(
            situacao(&papers, &h.0, &dados),
            Situacao {
                permissao: false,
                banco: Banco::Falta("criar o banco de certificados".into())
            }
        );

        let r = configurar(&papers, &h.0, &dados, &ja_ve).unwrap();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].1, Resultado::Uma(None));
        assert_eq!(situacao(&papers, &h.0, &dados).banco, Banco::Pronto);

        desfazer(&papers, &h.0, &dados).unwrap();
        assert!(!host::overrides(&h.0).join("org.gnome.Papers").exists());
        assert!(
            !host::var_app(&h.0, "org.gnome.Papers")
                .join(BANCO_POPPLER)
                .exists()
        );
        assert!(verificacao::ler_registros(&dados).is_empty());
    }

    #[test]
    fn permissao_que_ja_existia_nao_e_revogada() {
        let h = home("ja-tinha");
        let dados = h.0.join("dados");
        let arquivo = host::overrides(&h.0).join("org.gnome.Papers");
        fs::create_dir_all(host::overrides(&h.0)).unwrap();
        fs::write(&arquivo, "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11;\n").unwrap();

        configurar(&app("org.gnome.Papers"), &h.0, &dados, &ja_ve).unwrap();
        desfazer(&app("org.gnome.Papers"), &h.0, &dados).unwrap();
        assert!(permissao::tem(
            &fs::read_to_string(&arquivo).unwrap(),
            &permissao::Entrada::socket_do_sistema()
        ));
    }

    #[test]
    fn firefox_registra_onde_precisa_e_desfazer_tira() {
        let h = home("firefox");
        let dados = h.0.join("dados");
        let raiz = host::var_app(&h.0, "org.mozilla.firefox").join("config/mozilla/firefox");
        for p in ["a.default", "b.default-release"] {
            banco(&raiz.join(p));
        }
        let firefox = app("org.mozilla.firefox");
        assert_eq!(
            situacao(&firefox, &h.0, &dados).banco,
            Banco::Falta("verificar".into())
        );

        let r = configurar(&firefox, &h.0, &dados, &so_com_bloco).unwrap();
        assert_eq!(r.len(), 2);
        assert!(
            r.iter()
                .all(|(_, r)| *r == Resultado::Uma(Some(nss::CLIENTE_NO_SANDBOX.into())))
        );
        assert!(situacao(&firefox, &h.0, &dados).pronto());

        // Alguém tira o bloco depois: deixa de estar pronto.
        nss::editar(&raiz.join("a.default"), |t| nss::sem_modulo(t, nss::NOME)).unwrap();
        assert_eq!(
            situacao(&firefox, &h.0, &dados).banco,
            Banco::Falta("verificar".into())
        );

        desfazer(&firefox, &h.0, &dados).unwrap();
        for p in ["a.default", "b.default-release"] {
            let texto = nss::ler(&raiz.join(p));
            assert!(!nss::tem_modulo(&texto, nss::NOME));
            assert!(texto.contains("NSS Internal"));
        }
    }

    #[test]
    fn sem_poder_medir_fica_a_receita_padrao_marcada() {
        let h = home("sem-medir");
        let dados = h.0.join("dados");
        banco(&host::var_app(&h.0, "org.mozilla.firefox").join("config/mozilla/firefox/a.default"));
        let firefox = app("org.mozilla.firefox");
        let r = configurar(
            &firefox,
            &h.0,
            &dados,
            &verificacao::Indisponivel("sem sandbox".into()),
        )
        .unwrap();
        assert!(
            matches!(&r[0].1, Resultado::NaoVerificado { registro: Some(c), .. } if c == nss::CLIENTE_NO_SANDBOX)
        );
        assert_eq!(situacao(&firefox, &h.0, &dados).banco, Banco::NaoVerificado);
    }

    #[test]
    fn firefox_sem_perfil_pede_para_abrir_uma_vez() {
        let h = home("firefox-novo");
        assert_eq!(
            situacao(&app("org.mozilla.firefox"), &h.0, &h.0.join("dados")).banco,
            Banco::Falta("abra o Firefox uma vez para criar o perfil".into())
        );
    }

    #[test]
    fn chromium_tira_duplicado_de_outro_dono_e_preserva_o_nativo() {
        let h = home("chromium");
        let dados = h.0.join("dados");
        // O pacote do Chromium usa o ~/.pki/nssdb real, o mesmo dos nativos.
        let nssdb = h.0.join(".pki/nssdb");
        banco(&nssdb);
        nss::editar(&nssdb, |t| {
            nss::com_modulo(t, "Tessera (p11-kit)", nss::CLIENTE_NO_SANDBOX)
        })
        .unwrap();
        nss::editar(&nssdb, |t| {
            nss::com_modulo(t, nss::NOME_NATIVO, "/usr/lib/p11-kit-proxy.so")
        })
        .unwrap();
        let chromium = app("org.chromium.Chromium");
        assert!(!situacao(&chromium, &h.0, &dados).pronto());

        let r = configurar(&chromium, &h.0, &dados, &ja_ve).unwrap();
        assert_eq!(r[0], (nssdb.clone(), Resultado::Uma(None)));
        let texto = nss::ler(&nssdb);
        assert!(!texto.contains("Tessera"));
        assert!(
            nss::tem_modulo(&texto, nss::NOME_NATIVO),
            "a publicação nativa não pode sair"
        );
        assert!(situacao(&chromium, &h.0, &dados).pronto());

        desfazer(&chromium, &h.0, &dados).unwrap();
        assert!(nss::tem_modulo(&nss::ler(&nssdb), nss::NOME_NATIVO));
    }
}
