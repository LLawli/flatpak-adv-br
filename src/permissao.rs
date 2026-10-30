//! A permissão que um aplicativo Flatpak precisa para enxergar os tokens: o
//! socket do `p11-kit-server.socket` do usuário, que serve os módulos do
//! host (inclusive os do Âncora) e a confiança do sistema.
//!
//! Concedida editando o override do aplicativo, o mesmo arquivo INI que
//! `flatpak override --user` escreve. Desfazer não é `--nofilesystem` (grava
//! uma negação permanente) nem `--reset` (apaga o que outros programas
//! concederam): é tirar só a entrada que o Âncora pôs.

use gtk::glib;

/// A permissão, como aparece em `filesystems`.
pub const SOCKET_DO_SISTEMA: &str = "xdg-run/p11-kit/pkcs11";

const GRUPO: &str = "Context";
const CHAVE: &str = "filesystems";

fn carregar(texto: &str) -> Result<glib::KeyFile, String> {
    let kf = glib::KeyFile::new();
    if !texto.trim().is_empty() {
        kf.load_from_data(texto, glib::KeyFileFlags::KEEP_COMMENTS)
            .map_err(|e| e.to_string())?;
    }
    Ok(kf)
}

fn entradas(kf: &glib::KeyFile) -> Vec<String> {
    kf.string_list(GRUPO, CHAVE)
        .map(|l| l.iter().map(|s| s.to_string()).collect())
        .unwrap_or_default()
}

/// A entrada sem o modo (`:ro`, `:rw`, `:create`).
fn caminho(entrada: &str) -> &str {
    entrada.split(':').next().unwrap_or(entrada)
}

fn guardar(kf: &glib::KeyFile, lista: &[String]) -> Option<String> {
    if lista.is_empty() {
        let _ = kf.remove_key(GRUPO, CHAVE);
        if kf.keys(GRUPO).map(|k| k.is_empty()).unwrap_or(false) {
            let _ = kf.remove_group(GRUPO);
        }
    } else {
        // O formato de lista que o Flatpak escreve: cada item seguido de `;`.
        let valor: String = lista.iter().map(|e| format!("{e};")).collect();
        kf.set_value(GRUPO, CHAVE, &valor);
    }
    if kf.groups().is_empty() {
        None
    } else {
        Some(kf.to_data().to_string())
    }
}

/// O override já dá acesso ao socket.
pub fn tem(texto: &str) -> bool {
    carregar(texto)
        .map(|kf| {
            entradas(&kf)
                .iter()
                .any(|e| caminho(e) == SOCKET_DO_SISTEMA)
        })
        .unwrap_or(false)
}

/// O override com a permissão. Uma negação anterior (`!xdg-run/...`) sai.
pub fn conceder(texto: &str) -> Result<String, String> {
    let kf = carregar(texto)?;
    let negacao = format!("!{SOCKET_DO_SISTEMA}");
    let mut lista: Vec<String> = entradas(&kf)
        .into_iter()
        .filter(|e| caminho(e) != negacao)
        .collect();
    if !lista.iter().any(|e| caminho(e) == SOCKET_DO_SISTEMA) {
        lista.push(SOCKET_DO_SISTEMA.to_owned());
    }
    Ok(guardar(&kf, &lista).expect("a lista tem ao menos a permissão"))
}

/// O override sem a permissão; `None` quando não sobra nada e o arquivo deve
/// ser apagado.
pub fn revogar(texto: &str) -> Result<Option<String>, String> {
    let kf = carregar(texto)?;
    let lista: Vec<String> = entradas(&kf)
        .into_iter()
        .filter(|e| caminho(e) != SOCKET_DO_SISTEMA)
        .collect();
    Ok(guardar(&kf, &lista))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concede_num_override_vazio() {
        let novo = conceder("").unwrap();
        assert!(tem(&novo));
        assert!(novo.contains("[Context]"));
        assert!(novo.contains("filesystems=xdg-run/p11-kit/pkcs11;"));
    }

    #[test]
    fn concede_preservando_o_que_outros_programas_puseram() {
        let antes = "[Context]\nfilesystems=xdg-download;~/.keepassxc:ro;\nsockets=wayland;\n\n[Environment]\nFOO=1\n";
        let novo = conceder(antes).unwrap();
        assert!(tem(&novo));
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
    fn concede_troca_uma_negacao_anterior() {
        let novo = conceder("[Context]\nfilesystems=!xdg-run/p11-kit/pkcs11;\n").unwrap();
        assert!(tem(&novo));
        assert!(!novo.contains('!'), "{novo}");
    }

    #[test]
    fn conceder_de_novo_nao_duplica() {
        let uma = conceder("").unwrap();
        assert_eq!(conceder(&uma).unwrap(), uma);
    }

    #[test]
    fn com_modo_tambem_conta() {
        assert!(tem("[Context]\nfilesystems=xdg-run/p11-kit/pkcs11:rw;\n"));
        assert!(!tem("[Context]\nfilesystems=!xdg-run/p11-kit/pkcs11;\n"));
        assert!(!tem(""));
    }

    #[test]
    fn revogar_tira_so_a_permissao() {
        let antes = "[Context]\nfilesystems=xdg-download;xdg-run/p11-kit/pkcs11;\n";
        let depois = revogar(antes).unwrap().unwrap();
        assert!(!tem(&depois));
        assert!(depois.contains("xdg-download"));
        assert!(
            !depois.contains('!'),
            "revogar não pode virar negação:\n{depois}"
        );
    }

    #[test]
    fn revogar_a_unica_coisa_pede_apagar_o_arquivo() {
        assert_eq!(revogar(&conceder("").unwrap()).unwrap(), None);
    }

    #[test]
    fn revogar_mantem_outros_grupos() {
        let antes = "[Context]\nfilesystems=xdg-run/p11-kit/pkcs11;\n\n[Environment]\nFOO=1\n";
        let depois = revogar(antes).unwrap().unwrap();
        assert!(!depois.contains("[Context]"), "{depois}");
        assert!(depois.contains("FOO=1"));
    }
}
