//! Navegadores e leitores de PDF nativos, fora do Flatpak.
//!
//! Eles não passam pelo servidor do sistema: carregam o que está registrado
//! no banco NSS deles. Publicar é registrar ali o `p11-kit-proxy.so` do host,
//! que lê os `.module` do usuário e chega aos servidores do Âncora;
//! despublicar é tirar esse registro. É uma escolha da pessoa, por destino.
//!
//! O registro usa o caminho absoluto do proxy do host. Medido em 2026-10-30
//! (Arch): no host ele carrega e o token aparece uma vez; dentro de um
//! sandbox, que não tem esse caminho, o NSS o ignora em silêncio. Isso importa
//! porque o `~/.pki/nssdb` é lido também pelo Chromium Flatpak. No Debian e no
//! Ubuntu o caminho do host coincide com o do runtime, e esse Chromium
//! veria o token duas vezes.

use crate::nss;
use crate::verificacao::{self, Ambiente, Medidor, Raizes, Registro, Resultado};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    /// Os perfis do Firefox nativo.
    Firefox,
    /// O banco compartilhado do usuário: Chromium, Chrome, Brave, Edge e
    /// outros derivados nativos, e o poppler do Evince e do Okular nativos.
    Sistema,
}

impl Destino {
    pub const TODOS: [Destino; 2] = [Destino::Firefox, Destino::Sistema];

    pub fn id(self) -> &'static str {
        match self {
            Destino::Firefox => "firefox",
            Destino::Sistema => "sistema",
        }
    }

    pub fn nome(self) -> &'static str {
        match self {
            Destino::Firefox => "Firefox",
            Destino::Sistema => "Chromium, Chrome, Brave e outros",
        }
    }

    pub fn por_id(id: &str) -> Option<Destino> {
        Destino::TODOS.into_iter().find(|d| d.id() == id)
    }
}

/// Os bancos de um destino que existem. O Âncora não cria banco de
/// navegador: o navegador cria o dele na primeira vez que abre.
pub fn bancos(destino: Destino, home: &Path) -> Vec<PathBuf> {
    match destino {
        Destino::Firefox => [
            home.join(".mozilla/firefox"),
            home.join(".config/mozilla/firefox"),
        ]
        .iter()
        .flat_map(|r| nss::bancos(r))
        .collect(),
        // Desde o Chromium 146 o banco é `$XDG_DATA_HOME/pki/nssdb`, salvo se o
        // antigo existir: aí o antigo vale.
        Destino::Sistema => [home.join(".pki/nssdb"), home.join(".local/share/pki/nssdb")]
            .into_iter()
            .find(|b| b.join("cert9.db").is_file())
            .into_iter()
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Situacao {
    pub bancos: usize,
    /// Bancos em que a publicação foi decidida e continua valendo.
    pub publicados: usize,
    /// Desses, os que foram medidos (e não só configurados pelo padrão).
    pub verificados: usize,
    /// Algum perfil está aberto por um Firefox.
    pub em_uso: bool,
}

impl Situacao {
    pub fn publicado(&self) -> bool {
        self.bancos > 0 && self.publicados == self.bancos
    }
}

fn raizes(destino: Destino) -> Raizes {
    match destino {
        Destino::Firefox => Raizes::Automaticas,
        Destino::Sistema => Raizes::Chromium,
    }
}

pub fn situacao(destino: Destino, home: &Path, dados: &Path) -> Situacao {
    let bancos = bancos(destino, home);
    let registros = verificacao::ler_registros(dados);
    let valendo: Vec<_> = bancos
        .iter()
        .filter_map(|b| {
            registros
                .get(&(b.clone(), nss::NOME_NATIVO.to_owned()))
                .filter(|r| r.vale_para(b, nss::NOME_NATIVO))
        })
        .collect();
    Situacao {
        bancos: bancos.len(),
        publicados: valendo.len(),
        verificados: valendo.iter().filter(|r| r.verificado).count(),
        em_uso: bancos.iter().any(|b| nss::em_uso(b)),
    }
}

fn bancos_editaveis(destino: Destino, home: &Path) -> Result<Vec<PathBuf>, String> {
    let bancos = bancos(destino, home);
    if bancos.is_empty() {
        return Err(format!(
            "{}: nenhum banco; abra o navegador uma vez",
            destino.nome()
        ));
    }
    if bancos.iter().any(|b| nss::em_uso(b)) {
        return Err(format!("feche o {} antes", destino.nome()));
    }
    Ok(bancos)
}

/// Publica em cada banco do destino da forma que deixa o token aparecer uma
/// vez, medido no host: sem registro (se o sistema já leva o token até lá),
/// pelo caminho absoluto do proxy, ou pelo nome curto. Sem poder medir, fica o
/// caminho absoluto.
pub fn publicar(
    destino: Destino,
    home: &Path,
    dados: &Path,
    proxy: &str,
    medidor: &dyn Medidor,
) -> Result<Vec<(PathBuf, Resultado)>, String> {
    let opcoes = [
        None,
        Some(proxy.to_owned()),
        Some("p11-kit-proxy.so".to_owned()),
    ];
    let padrao = Some(proxy.to_owned());
    let mut saida = Vec::new();
    for banco in bancos_editaveis(destino, home)? {
        let resultado = verificacao::garantir_um_no_banco(
            &banco,
            nss::NOME_NATIVO,
            medidor,
            Ambiente::Host,
            raizes(destino),
            &opcoes,
            &padrao,
        )?;
        verificacao::gravar_registro(
            dados,
            &banco,
            nss::NOME_NATIVO,
            Some(Registro::de(&resultado)),
        )?;
        saida.push((banco, resultado));
    }
    Ok(saida)
}

pub fn despublicar(destino: Destino, home: &Path, dados: &Path) -> Result<(), String> {
    for banco in bancos_editaveis(destino, home)? {
        nss::editar(&banco, |t| nss::sem_modulo(t, nss::NOME_NATIVO))
            .map_err(|e| format!("{}: {e}", banco.display()))?;
        verificacao::gravar_registro(dados, &banco, nss::NOME_NATIVO, None)?;
    }
    Ok(())
}

/// Onde as distribuições põem o proxy, na ordem em que são procurados:
/// Fedora e openSUSE, Debian e Ubuntu, Arch.
fn candidatos() -> [String; 3] {
    [
        "/usr/lib64/p11-kit-proxy.so".to_owned(),
        format!(
            "/usr/lib/{}-linux-gnu/p11-kit-proxy.so",
            std::env::consts::ARCH
        ),
        "/usr/lib/p11-kit-proxy.so".to_owned(),
    ]
}

/// O primeiro candidato que aparece na lista do que existe no host.
fn escolher(existentes: &str) -> Option<String> {
    let existentes: Vec<&str> = existentes.lines().map(str::trim).collect();
    candidatos()
        .into_iter()
        .find(|c| existentes.contains(&c.as_str()))
}

/// O caminho do `p11-kit-proxy.so` no host, perguntado ao host.
pub fn proxy_do_host() -> Result<String, String> {
    let saida = Command::new("flatpak-spawn")
        .args(["--host", "ls", "-1"])
        .args(candidatos())
        .output()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    // `ls` sai com erro pelos que faltam; o que importa é a lista dos que há.
    escolher(&String::from_utf8_lossy(&saida.stdout))
        .ok_or_else(|| "o p11-kit-proxy.so não foi encontrado no sistema".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Home(PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn home(nome: &str) -> Home {
        let p = std::env::temp_dir().join(format!("ancora-nativos-{}-{nome}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Home(p)
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
    fn escolhe_o_proxy_na_ordem_das_distribuicoes() {
        assert_eq!(
            escolher("/usr/lib/p11-kit-proxy.so\n"),
            Some("/usr/lib/p11-kit-proxy.so".into())
        );
        assert_eq!(
            escolher("/usr/lib/p11-kit-proxy.so\n/usr/lib64/p11-kit-proxy.so\n"),
            Some("/usr/lib64/p11-kit-proxy.so".into())
        );
        assert_eq!(escolher(""), None);
        assert_eq!(escolher("/opt/outro/p11-kit-proxy.so\n"), None);
    }

    #[test]
    fn o_banco_antigo_do_chromium_vale_sobre_o_novo() {
        let h = home("chromium");
        banco(&h.0.join(".local/share/pki/nssdb"));
        assert_eq!(
            bancos(Destino::Sistema, &h.0),
            vec![h.0.join(".local/share/pki/nssdb")]
        );
        banco(&h.0.join(".pki/nssdb"));
        assert_eq!(bancos(Destino::Sistema, &h.0), vec![h.0.join(".pki/nssdb")]);
    }

    /// Um host como o Arch: o token só aparece com o bloco do Âncora.
    fn como_o_arch(_: Ambiente, banco: &Path, _: Raizes) -> Result<verificacao::Contagem, String> {
        let vezes = usize::from(nss::tem_modulo(&nss::ler(banco), nss::NOME_NATIVO));
        Ok(verificacao::Contagem {
            vezes,
            modulos: vec![nss::NOME_NATIVO.into(); vezes],
        })
    }

    #[test]
    fn publica_em_todos_os_perfis_do_firefox_e_despublica() {
        let h = home("firefox");
        let dados = h.0.join("dados");
        banco(&h.0.join(".mozilla/firefox/a.default"));
        banco(&h.0.join(".config/mozilla/firefox/b.default-release"));
        assert_eq!(
            situacao(Destino::Firefox, &h.0, &dados),
            Situacao {
                bancos: 2,
                publicados: 0,
                verificados: 0,
                em_uso: false
            }
        );

        let r = publicar(
            Destino::Firefox,
            &h.0,
            &dados,
            "/usr/lib/p11-kit-proxy.so",
            &como_o_arch,
        )
        .unwrap();
        assert!(
            r.iter()
                .all(|(_, r)| *r == Resultado::Uma(Some("/usr/lib/p11-kit-proxy.so".into())))
        );
        let s = situacao(Destino::Firefox, &h.0, &dados);
        assert!(s.publicado() && s.verificados == 2);
        let texto = nss::ler(&h.0.join(".mozilla/firefox/a.default"));
        assert!(texto.contains("library=/usr/lib/p11-kit-proxy.so\nname=Âncora (sistema)"));

        despublicar(Destino::Firefox, &h.0, &dados).unwrap();
        assert_eq!(situacao(Destino::Firefox, &h.0, &dados).publicados, 0);
        assert!(nss::ler(&h.0.join(".mozilla/firefox/a.default")).contains("NSS Internal"));
    }

    #[test]
    fn sistema_que_ja_leva_o_token_nao_ganha_registro() {
        // Um Fedora com o proxy no NSS do sistema: sem o Âncora já é 1.
        let h = home("fedora");
        let dados = h.0.join("dados");
        banco(&h.0.join(".pki/nssdb"));
        let ja_uma = |_: Ambiente, banco: &Path, _: Raizes| {
            let vezes = 1 + usize::from(nss::tem_modulo(&nss::ler(banco), nss::NOME_NATIVO));
            Ok(verificacao::Contagem {
                vezes,
                modulos: vec![],
            })
        };
        let r = publicar(
            Destino::Sistema,
            &h.0,
            &dados,
            "/usr/lib64/p11-kit-proxy.so",
            &ja_uma,
        )
        .unwrap();
        assert_eq!(r[0].1, Resultado::Uma(None));
        assert!(!nss::tem_modulo(
            &nss::ler(&h.0.join(".pki/nssdb")),
            nss::NOME_NATIVO
        ));
        assert!(situacao(Destino::Sistema, &h.0, &dados).publicado());
    }

    #[test]
    fn mexer_no_banco_depois_desfaz_o_publicado() {
        let h = home("mexido");
        let dados = h.0.join("dados");
        banco(&h.0.join(".pki/nssdb"));
        publicar(
            Destino::Sistema,
            &h.0,
            &dados,
            "/usr/lib/p11-kit-proxy.so",
            &como_o_arch,
        )
        .unwrap();
        assert!(situacao(Destino::Sistema, &h.0, &dados).publicado());
        nss::editar(&h.0.join(".pki/nssdb"), |t| {
            nss::sem_modulo(t, nss::NOME_NATIVO)
        })
        .unwrap();
        assert!(!situacao(Destino::Sistema, &h.0, &dados).publicado());
    }

    #[test]
    fn sem_banco_nao_cria_nada_e_explica() {
        let h = home("vazio");
        let erro = publicar(
            Destino::Sistema,
            &h.0,
            &h.0.join("dados"),
            "/usr/lib/p11-kit-proxy.so",
            &como_o_arch,
        )
        .unwrap_err();
        assert!(erro.contains("abra o navegador"), "{erro}");
        assert!(!h.0.join(".pki").exists());
        assert!(!situacao(Destino::Sistema, &h.0, &h.0.join("dados")).publicado());
    }
}
