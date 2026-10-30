//! Permissões que o Âncora dá a outros aplicativos Flatpak: o socket do
//! `p11-kit-server.socket` do usuário (para enxergar os tokens) e, para os
//! assinadores, ler a pasta do programa e o X11.
//!
//! Concedidas editando o override do aplicativo, o mesmo arquivo INI que
//! `flatpak override --user` escreve. Desfazer não é `--no<permissão>` (grava
//! uma negação permanente) nem `--reset` (apaga o que outros programas
//! concederam): é tirar só a entrada que o Âncora pôs, e só se foi ele que a
//! pôs. O que ele pôs fica anotado em `concessoes`, nos dados do Âncora.

use crate::host;
use gtk::glib;
use std::fs;
use std::path::{Path, PathBuf};

/// O socket do servidor do sistema, como aparece em `filesystems`.
pub const SOCKET_DO_SISTEMA: &str = "xdg-run/p11-kit/pkcs11";

const GRUPO: &str = "Context";

/// Uma entrada de override: a chave (`filesystems`, `sockets`) e o valor.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Entrada {
    pub chave: &'static str,
    pub valor: String,
}

impl Entrada {
    pub fn arquivos(caminho: impl Into<String>) -> Entrada {
        Entrada {
            chave: "filesystems",
            valor: caminho.into(),
        }
    }

    pub fn socket(nome: impl Into<String>) -> Entrada {
        Entrada {
            chave: "sockets",
            valor: nome.into(),
        }
    }

    pub fn socket_do_sistema() -> Entrada {
        Entrada::arquivos(SOCKET_DO_SISTEMA)
    }

    /// O valor sem o modo (`:ro`, `:rw`, `:create`).
    fn base(&self) -> &str {
        caminho(&self.valor)
    }
}

fn carregar(texto: &str) -> Result<glib::KeyFile, String> {
    let kf = glib::KeyFile::new();
    if !texto.trim().is_empty() {
        kf.load_from_data(texto, glib::KeyFileFlags::KEEP_COMMENTS)
            .map_err(|e| e.to_string())?;
    }
    Ok(kf)
}

fn lista(kf: &glib::KeyFile, chave: &str) -> Vec<String> {
    kf.string_list(GRUPO, chave)
        .map(|l| l.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

fn caminho(entrada: &str) -> &str {
    entrada.split(':').next().unwrap_or(entrada)
}

fn guardar(kf: &glib::KeyFile, chave: &str, itens: &[String]) -> Option<String> {
    if itens.is_empty() {
        let _ = kf.remove_key(GRUPO, chave);
        if kf.keys(GRUPO).map(|k| k.is_empty()).unwrap_or(false) {
            let _ = kf.remove_group(GRUPO);
        }
    } else {
        // O formato de lista que o Flatpak escreve: cada item seguido de `;`.
        let valor: String = itens.iter().map(|e| format!("{e};")).collect();
        kf.set_value(GRUPO, chave, &valor);
    }
    if kf.groups().is_empty() {
        None
    } else {
        Some(kf.to_data().to_string())
    }
}

/// O override já tem a entrada (com qualquer modo).
pub fn tem(texto: &str, entrada: &Entrada) -> bool {
    carregar(texto)
        .map(|kf| {
            lista(&kf, entrada.chave)
                .iter()
                .any(|e| caminho(e) == entrada.base())
        })
        .unwrap_or(false)
}

/// O override com a entrada. Uma negação anterior (`!x`) sai.
pub fn conceder(texto: &str, entrada: &Entrada) -> Result<String, String> {
    let kf = carregar(texto)?;
    let negacao = format!("!{}", entrada.base());
    let mut itens: Vec<String> = lista(&kf, entrada.chave)
        .into_iter()
        .filter(|e| caminho(e) != negacao)
        .collect();
    if !itens.iter().any(|e| caminho(e) == entrada.base()) {
        itens.push(entrada.valor.clone());
    }
    Ok(guardar(&kf, entrada.chave, &itens).expect("a lista tem ao menos a entrada"))
}

/// O override sem a entrada; `None` quando não sobra nada e o arquivo deve
/// ser apagado.
pub fn revogar(texto: &str, entrada: &Entrada) -> Result<Option<String>, String> {
    let kf = carregar(texto)?;
    let itens: Vec<String> = lista(&kf, entrada.chave)
        .into_iter()
        .filter(|e| caminho(e) != entrada.base())
        .collect();
    Ok(guardar(&kf, entrada.chave, &itens))
}

fn arquivo_override(home: &Path, app: &str) -> PathBuf {
    host::overrides(home).join(app)
}

fn arquivo_concessoes(dados: &Path) -> PathBuf {
    dados.join("concessoes")
}

/// O que o Âncora concedeu, uma linha por entrada: aplicativo, chave e valor,
/// separados por tabulação.
fn ler_concessoes(dados: &Path) -> Vec<(String, Entrada)> {
    fs::read_to_string(arquivo_concessoes(dados))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut c = l.splitn(3, '\t');
            let app = c.next()?.to_owned();
            let chave = match c.next()? {
                "filesystems" => "filesystems",
                "sockets" => "sockets",
                _ => return None,
            };
            let valor = c.next()?.to_owned();
            Some((app, Entrada { chave, valor }))
        })
        .collect()
}

fn gravar_concessoes(dados: &Path, todas: &[(String, Entrada)]) -> Result<(), String> {
    fs::create_dir_all(dados).map_err(|e| format!("{}: {e}", dados.display()))?;
    let texto: String = todas
        .iter()
        .map(|(app, e)| format!("{app}\t{}\t{}\n", e.chave, e.valor))
        .collect();
    fs::write(arquivo_concessoes(dados), texto).map_err(|e| e.to_string())
}

pub fn tem_em(home: &Path, app: &str, entrada: &Entrada) -> bool {
    tem(
        &fs::read_to_string(arquivo_override(home, app)).unwrap_or_default(),
        entrada,
    )
}

/// Concede a entrada ao aplicativo, se ainda não estiver lá, e anota que foi
/// o Âncora.
pub fn conceder_a(home: &Path, dados: &Path, app: &str, entrada: &Entrada) -> Result<(), String> {
    let arquivo = arquivo_override(home, app);
    let atual = fs::read_to_string(&arquivo).unwrap_or_default();
    if tem(&atual, entrada) {
        return Ok(());
    }
    if let Some(pai) = arquivo.parent() {
        fs::create_dir_all(pai).map_err(|e| format!("{}: {e}", pai.display()))?;
    }
    fs::write(&arquivo, conceder(&atual, entrada)?)
        .map_err(|e| format!("{}: {e}", arquivo.display()))?;
    let mut todas = ler_concessoes(dados);
    todas.push((app.to_owned(), entrada.clone()));
    gravar_concessoes(dados, &todas)
}

/// Tira a entrada do aplicativo, só se foi o Âncora que a concedeu.
pub fn revogar_de(home: &Path, dados: &Path, app: &str, entrada: &Entrada) -> Result<(), String> {
    let mut todas = ler_concessoes(dados);
    let Some(i) = todas.iter().position(|(a, e)| a == app && e == entrada) else {
        return Ok(());
    };
    let arquivo = arquivo_override(home, app);
    let atual = fs::read_to_string(&arquivo).unwrap_or_default();
    match revogar(&atual, entrada)? {
        Some(novo) => {
            fs::write(&arquivo, novo).map_err(|e| format!("{}: {e}", arquivo.display()))?
        }
        None => match fs::remove_file(&arquivo) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("{}: {e}", arquivo.display()));
            }
            _ => {}
        },
    }
    todas.remove(i);
    gravar_concessoes(dados, &todas)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn socket() -> Entrada {
        Entrada::socket_do_sistema()
    }

    #[test]
    fn concede_num_override_vazio() {
        let novo = conceder("", &socket()).unwrap();
        assert!(tem(&novo, &socket()));
        assert!(novo.contains("[Context]"));
        assert!(novo.contains("filesystems=xdg-run/p11-kit/pkcs11;"));
    }

    #[test]
    fn concede_preservando_o_que_outros_programas_puseram() {
        let antes = "[Context]\nfilesystems=xdg-download;~/.keepassxc:ro;\nsockets=wayland;\n\n[Environment]\nFOO=1\n";
        let novo = conceder(antes, &socket()).unwrap();
        assert!(tem(&novo, &socket()));
        for resto in [
            "xdg-download",
            "~/.keepassxc:ro",
            "sockets=wayland",
            "FOO=1",
        ] {
            assert!(novo.contains(resto), "perdeu {resto}:\n{novo}");
        }
    }

    #[test]
    fn concede_um_socket_ao_lado_dos_outros() {
        let novo = conceder("[Context]\nsockets=wayland;\n", &Entrada::socket("x11")).unwrap();
        assert!(novo.contains("sockets=wayland;x11;"), "{novo}");
    }

    #[test]
    fn concede_troca_uma_negacao_anterior() {
        let novo = conceder(
            "[Context]\nfilesystems=!xdg-run/p11-kit/pkcs11;\n",
            &socket(),
        )
        .unwrap();
        assert!(tem(&novo, &socket()));
        assert!(!novo.contains('!'), "{novo}");
    }

    #[test]
    fn conceder_de_novo_nao_duplica() {
        let uma = conceder("", &socket()).unwrap();
        assert_eq!(conceder(&uma, &socket()).unwrap(), uma);
    }

    #[test]
    fn com_modo_tambem_conta() {
        assert!(tem(
            "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11:rw;\n",
            &socket()
        ));
        assert!(!tem(
            "[Context]\nfilesystems=!xdg-run/p11-kit/pkcs11;\n",
            &socket()
        ));
        assert!(!tem("", &socket()));
        let ro = Entrada::arquivos("/var/lib/flatpak/runtime/x:ro");
        assert!(tem(
            "[Context]\nfilesystems=/var/lib/flatpak/runtime/x:ro;\n",
            &ro
        ));
    }

    #[test]
    fn revogar_tira_so_a_entrada() {
        let antes = "[Context]\nfilesystems=xdg-download;xdg-run/p11-kit/pkcs11;\n";
        let depois = revogar(antes, &socket()).unwrap().unwrap();
        assert!(!tem(&depois, &socket()));
        assert!(depois.contains("xdg-download"));
        assert!(
            !depois.contains('!'),
            "revogar não pode virar negação:\n{depois}"
        );
    }

    #[test]
    fn revogar_a_unica_coisa_pede_apagar_o_arquivo() {
        assert_eq!(
            revogar(&conceder("", &socket()).unwrap(), &socket()).unwrap(),
            None
        );
    }

    #[test]
    fn revogar_mantem_outros_grupos() {
        let antes = "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11;\n\n[Environment]\nFOO=1\n";
        let depois = revogar(antes, &socket()).unwrap().unwrap();
        assert!(!depois.contains("[Context]"), "{depois}");
        assert!(depois.contains("FOO=1"));
    }

    #[test]
    fn so_revoga_o_que_o_ancora_concedeu() {
        let base = std::env::temp_dir().join(format!("ancora-perm-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let (home, dados) = (base.join("home"), base.join("dados"));
        let x11 = Entrada::socket("x11");
        // O Firefox já tinha o socket do sistema, posto por outro programa.
        fs::create_dir_all(host::overrides(&home)).unwrap();
        fs::write(
            arquivo_override(&home, "f"),
            "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11;\n",
        )
        .unwrap();

        conceder_a(&home, &dados, "f", &socket()).unwrap();
        conceder_a(&home, &dados, "f", &x11).unwrap();
        assert!(tem_em(&home, "f", &x11));
        revogar_de(&home, &dados, "f", &socket()).unwrap();
        revogar_de(&home, &dados, "f", &x11).unwrap();
        assert!(tem_em(&home, "f", &socket()), "não era do Âncora");
        assert!(!tem_em(&home, "f", &x11));
        let _ = fs::remove_dir_all(&base);
    }
}
