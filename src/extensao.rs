//! O contrato entre o Âncora e as extensões dele.
//!
//! Cada extensão é um ref Flatpak `dev.lukakuuhaku.Ancora.<Família>.<Nome>`,
//! montado em `/app/extensoes/<família>/<Nome>`, com um `ancora.toml` na raiz
//! dizendo o que ela é e o que oferece. O Âncora não tem tabela de extensões:
//! tudo o que a interface mostra de uma extensão sai deste arquivo, e some
//! quando ela é removida. O formato está em `docs/extensoes.md`.
//!
//! O descritor é declarativo de propósito. O projeto antigo rodava um
//! `preparar.sh` de cada extensão antes de carregar o driver; aqui não se
//! executa nada da extensão para saber o que ela é.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// A versão do contrato que este Âncora entende. Um descritor com número
/// maior foi feito para um Âncora mais novo.
pub const CONTRATO: u32 = 1;

/// O nome do descritor na raiz de cada extensão.
pub const ARQUIVO: &str = "ancora.toml";

/// Onde o manifesto monta as extensões.
pub const RAIZ: &str = "/app/extensoes";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Familia {
    Driver,
    Assinador,
    App,
}

impl Familia {
    pub const TODAS: [Familia; 3] = [Familia::Driver, Familia::Assinador, Familia::App];

    /// O subdiretório de `RAIZ` onde as extensões desta família são montadas.
    pub fn diretorio(self) -> &'static str {
        match self {
            Familia::Driver => "driver",
            Familia::Assinador => "assinador",
            Familia::App => "app",
        }
    }

    /// O ponto de extensão no manifesto: `dev.lukakuuhaku.Ancora.<isto>`.
    pub fn ponto(self) -> &'static str {
        match self {
            Familia::Driver => "Driver",
            Familia::Assinador => "Assinador",
            Familia::App => "App",
        }
    }
}

/// Uma biblioteca PKCS#11 a publicar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pkcs11 {
    pub modulo: PathBuf,
    /// Vai para o `LD_LIBRARY_PATH` do processo que carrega o módulo.
    pub bibliotecas: Vec<PathBuf>,
    /// Variáveis do processo que carrega o módulo.
    pub ambiente: BTreeMap<String, String>,
}

/// Um programa que o navegador chama por native messaging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assinador {
    /// O nome do host de native messaging (`com.lacunasoftware.webpki`).
    pub host: String,
    pub executavel: PathBuf,
    /// Os manifestos do fabricante, um por família de navegador. O Âncora
    /// troca só o campo `path` ao publicar.
    pub manifesto_firefox: Option<PathBuf>,
    pub manifesto_chromium: Option<PathBuf>,
    pub ambiente: BTreeMap<String, String>,
}

/// Um programa com janela que a pessoa abre: o Token Administration do
/// SafeSign, o aplicativo do SerproID, o PJeOffice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ferramenta {
    pub nome: String,
    pub executavel: PathBuf,
    pub argumentos: Vec<String>,
    pub ambiente: BTreeMap<String, String>,
    /// Ganha atalho no menu de aplicativos do sistema.
    pub atalho: bool,
}

/// O que um `ancora.toml` válido descreve, com os caminhos já resolvidos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descritor {
    pub nome: String,
    pub fabricante: String,
    pub descricao: String,
    pub icone: Option<PathBuf>,
    /// Diretórios que precisam existir na home antes de a extensão ser usada.
    pub criar_diretorios: Vec<PathBuf>,
    pub pkcs11: Vec<Pkcs11>,
    pub assinadores: Vec<Assinador>,
    pub ferramentas: Vec<Ferramenta>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Erro {
    Ler(String),
    Formato(String),
    /// O descritor foi feito para um Âncora mais novo.
    ContratoNovo(u32),
    /// Um caminho absoluto, com `..`, ou com `~/` onde não cabe.
    Caminho {
        campo: &'static str,
        valor: String,
    },
    /// A extensão não oferece o que a família dela exige.
    Vazia(Familia),
}

impl fmt::Display for Erro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Erro::Ler(e) => write!(f, "não foi possível ler o {ARQUIVO}: {e}"),
            Erro::Formato(e) => write!(f, "{ARQUIVO} inválido: {e}"),
            Erro::ContratoNovo(n) => write!(
                f,
                "feita para uma versão mais nova do Âncora (contrato {n}, este entende até {CONTRATO})"
            ),
            Erro::Caminho { campo, valor } => {
                write!(f, "caminho fora da extensão em {campo}: {valor}")
            }
            Erro::Vazia(Familia::Driver) => write!(f, "driver sem nenhum módulo [[pkcs11]]"),
            Erro::Vazia(Familia::Assinador) => {
                write!(f, "assinador sem nenhum [[assinador]]")
            }
            Erro::Vazia(Familia::App) => write!(f, "aplicativo sem nenhuma [[ferramenta]]"),
        }
    }
}

impl std::error::Error for Erro {}

/// Uma extensão montada, com o descritor ou o motivo de ele não valer. Uma
/// extensão quebrada continua na lista: a interface precisa mostrá-la para
/// que a pessoa possa atualizá-la ou removê-la.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Encontrada {
    /// O último componente do ref (`SafeSign`), que é o nome do diretório.
    pub id: String,
    pub familia: Familia,
    pub diretorio: PathBuf,
    pub descritor: Result<Descritor, Erro>,
}

/// As extensões montadas sob `raiz`, em ordem de família e de id. Diretório
/// sem `ancora.toml` não é extensão do Âncora e fica de fora.
pub fn encontrar(raiz: &Path, home: &Path) -> Vec<Encontrada> {
    let mut saida = Vec::new();
    for familia in Familia::TODAS {
        let Ok(entradas) = fs::read_dir(raiz.join(familia.diretorio())) else {
            continue;
        };
        let mut diretorios: Vec<PathBuf> = entradas
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.join(ARQUIVO).is_file())
            .collect();
        diretorios.sort();
        for diretorio in diretorios {
            let id = diretorio
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let descritor = ler(&diretorio, familia, home);
            saida.push(Encontrada {
                id,
                familia,
                diretorio,
                descritor,
            });
        }
    }
    saida
}

/// Lê e valida o `ancora.toml` de uma extensão.
pub fn ler(diretorio: &Path, familia: Familia, home: &Path) -> Result<Descritor, Erro> {
    let texto =
        fs::read_to_string(diretorio.join(ARQUIVO)).map_err(|e| Erro::Ler(e.to_string()))?;
    interpretar(&texto, diretorio, familia, home)
}

/// Valida um descritor e resolve os caminhos dele contra `diretorio` (os da
/// extensão) e `home` (os que começam com `~/`).
pub fn interpretar(
    texto: &str,
    diretorio: &Path,
    familia: Familia,
    home: &Path,
) -> Result<Descritor, Erro> {
    // O número do contrato é lido antes do resto: um campo que este Âncora
    // não conhece, num descritor mais novo, é "atualize o Âncora", não
    // "arquivo inválido".
    #[derive(Deserialize)]
    struct SoContrato {
        contrato: u32,
    }
    let SoContrato { contrato } =
        toml::from_str(texto).map_err(|e| Erro::Formato(e.message().to_owned()))?;
    if contrato > CONTRATO {
        return Err(Erro::ContratoNovo(contrato));
    }

    let a: arquivo::Arquivo =
        toml::from_str(texto).map_err(|e| Erro::Formato(e.message().to_owned()))?;
    let r = Resolvedor { diretorio, home };

    let d = Descritor {
        nome: a.nome,
        fabricante: a.fabricante,
        descricao: a.descricao,
        icone: a.icone.map(|c| r.dentro("icone", &c)).transpose()?,
        criar_diretorios: a
            .criar_diretorios
            .iter()
            .map(|c| r.na_home("criar_diretorios", c))
            .collect::<Result<_, _>>()?,
        pkcs11: a
            .pkcs11
            .into_iter()
            .map(|p| {
                Ok(Pkcs11 {
                    modulo: r.dentro("pkcs11.modulo", &p.modulo)?,
                    bibliotecas: p
                        .bibliotecas
                        .iter()
                        .map(|c| r.dentro("pkcs11.bibliotecas", c))
                        .collect::<Result<_, _>>()?,
                    ambiente: r.ambiente(p.ambiente),
                })
            })
            .collect::<Result<_, Erro>>()?,
        assinadores: a
            .assinador
            .into_iter()
            .map(|s| {
                Ok(Assinador {
                    host: s.host,
                    executavel: r.dentro("assinador.executavel", &s.executavel)?,
                    manifesto_firefox: s
                        .manifesto_firefox
                        .map(|c| r.dentro("assinador.manifesto_firefox", &c))
                        .transpose()?,
                    manifesto_chromium: s
                        .manifesto_chromium
                        .map(|c| r.dentro("assinador.manifesto_chromium", &c))
                        .transpose()?,
                    ambiente: r.ambiente(s.ambiente),
                })
            })
            .collect::<Result<_, Erro>>()?,
        ferramentas: a
            .ferramenta
            .into_iter()
            .map(|t| {
                Ok(Ferramenta {
                    nome: t.nome,
                    executavel: r.dentro("ferramenta.executavel", &t.executavel)?,
                    argumentos: t.argumentos,
                    ambiente: r.ambiente(t.ambiente),
                    atalho: t.atalho,
                })
            })
            .collect::<Result<_, Erro>>()?,
    };

    let oferece = match familia {
        Familia::Driver => !d.pkcs11.is_empty(),
        Familia::Assinador => !d.assinadores.is_empty(),
        Familia::App => !d.ferramentas.is_empty(),
    };
    if !oferece {
        return Err(Erro::Vazia(familia));
    }
    Ok(d)
}

struct Resolvedor<'a> {
    diretorio: &'a Path,
    home: &'a Path,
}

impl Resolvedor<'_> {
    /// Um caminho relativo que não sai do diretório da extensão.
    fn dentro(&self, campo: &'static str, valor: &str) -> Result<PathBuf, Erro> {
        let relativo = Path::new(valor);
        let preso = !valor.is_empty()
            && relativo
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !preso {
            return Err(Erro::Caminho {
                campo,
                valor: valor.to_owned(),
            });
        }
        Ok(self.diretorio.join(relativo))
    }

    /// Um caminho `~/...` que não sai da home.
    fn na_home(&self, campo: &'static str, valor: &str) -> Result<PathBuf, Erro> {
        let erro = || Erro::Caminho {
            campo,
            valor: valor.to_owned(),
        };
        let resto = valor.strip_prefix("~/").ok_or_else(erro)?;
        let relativo = Path::new(resto);
        let preso = !resto.is_empty()
            && relativo
                .components()
                .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !preso {
            return Err(erro());
        }
        Ok(self.home.join(relativo))
    }

    /// Nos valores do ambiente, `~/` no começo é a home.
    fn ambiente(&self, ambiente: BTreeMap<String, String>) -> BTreeMap<String, String> {
        ambiente
            .into_iter()
            .map(|(k, v)| match v.strip_prefix("~/") {
                Some(resto) => (k, self.home.join(resto).to_string_lossy().into_owned()),
                None => (k, v),
            })
            .collect()
    }
}

/// O formato do arquivo, como está escrito. Campo desconhecido é erro: um
/// erro de digitação num descritor viraria um campo ignorado em silêncio.
mod arquivo {
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Arquivo {
        #[allow(dead_code)]
        pub contrato: u32,
        pub nome: String,
        #[serde(default)]
        pub fabricante: String,
        #[serde(default)]
        pub descricao: String,
        pub icone: Option<String>,
        #[serde(default)]
        pub criar_diretorios: Vec<String>,
        #[serde(default)]
        pub pkcs11: Vec<Pkcs11>,
        #[serde(default)]
        pub assinador: Vec<Assinador>,
        #[serde(default)]
        pub ferramenta: Vec<Ferramenta>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Pkcs11 {
        pub modulo: String,
        #[serde(default)]
        pub bibliotecas: Vec<String>,
        #[serde(default)]
        pub ambiente: BTreeMap<String, String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Assinador {
        pub host: String,
        pub executavel: String,
        pub manifesto_firefox: Option<String>,
        pub manifesto_chromium: Option<String>,
        #[serde(default)]
        pub ambiente: BTreeMap<String, String>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct Ferramenta {
        pub nome: String,
        pub executavel: String,
        #[serde(default)]
        pub argumentos: Vec<String>,
        #[serde(default)]
        pub ambiente: BTreeMap<String, String>,
        #[serde(default)]
        pub atalho: bool,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIR: &str = "/app/extensoes/driver/SafeSign";
    const HOME: &str = "/home/pessoa";

    fn driver(texto: &str) -> Result<Descritor, Erro> {
        interpretar(texto, Path::new(DIR), Familia::Driver, Path::new(HOME))
    }

    const SAFESIGN: &str = r#"
        contrato = 1
        nome = "SafeSign IC"
        fabricante = "A.E.T. Europe"
        descricao = "Tokens GD Burti e outros SafeSign"
        icone = "icone.svg"

        [[pkcs11]]
        modulo = "extra/usr/lib/libaetpkss.so"
        bibliotecas = ["extra/usr/lib", "gdbm/lib"]

        [[ferramenta]]
        nome = "Token Administration"
        executavel = "extra/usr/bin/tokenadmin"
    "#;

    #[test]
    fn um_driver_completo_resolve_os_caminhos_dentro_da_extensao() {
        let d = driver(SAFESIGN).unwrap();
        assert_eq!(d.nome, "SafeSign IC");
        assert_eq!(d.fabricante, "A.E.T. Europe");
        assert_eq!(d.icone, Some(PathBuf::from(DIR).join("icone.svg")));
        assert_eq!(
            d.pkcs11,
            vec![Pkcs11 {
                modulo: PathBuf::from(DIR).join("extra/usr/lib/libaetpkss.so"),
                bibliotecas: vec![
                    PathBuf::from(DIR).join("extra/usr/lib"),
                    PathBuf::from(DIR).join("gdbm/lib"),
                ],
                ambiente: BTreeMap::new(),
            }]
        );
        assert_eq!(d.ferramentas.len(), 1);
        assert_eq!(
            d.ferramentas[0].executavel,
            PathBuf::from(DIR).join("extra/usr/bin/tokenadmin")
        );
        assert!(!d.ferramentas[0].atalho);
    }

    #[test]
    fn til_vira_a_home_no_ambiente_e_nos_diretorios_a_criar() {
        let d = driver(
            r#"
            contrato = 1
            nome = "SerproID"
            criar_diretorios = ["~/.config/serproid/certificados"]
            [[pkcs11]]
            modulo = "extra/libserproidp11.so"
            ambiente = { XDG_CONFIG_HOME = "~/.config", LD_PRELOAD = "libgcc_s.so.1" }
        "#,
        )
        .unwrap();
        assert_eq!(
            d.criar_diretorios,
            vec![PathBuf::from("/home/pessoa/.config/serproid/certificados")]
        );
        assert_eq!(
            d.pkcs11[0].ambiente["XDG_CONFIG_HOME"],
            "/home/pessoa/.config"
        );
        assert_eq!(d.pkcs11[0].ambiente["LD_PRELOAD"], "libgcc_s.so.1");
    }

    #[test]
    fn caminho_que_sai_da_extensao_e_recusado() {
        for modulo in [
            "/usr/lib/libx.so",
            "../Outro/libx.so",
            "extra/../../libx.so",
            "",
        ] {
            let texto = format!("contrato = 1\nnome = \"X\"\n[[pkcs11]]\nmodulo = \"{modulo}\"\n");
            assert_eq!(
                driver(&texto),
                Err(Erro::Caminho {
                    campo: "pkcs11.modulo",
                    valor: modulo.to_owned()
                }),
                "{modulo:?} deveria ser recusado"
            );
        }
    }

    #[test]
    fn diretorio_a_criar_precisa_ficar_na_home() {
        for dir in ["/etc/x", ".config/x", "~/../x", "~/"] {
            let texto = format!(
                "contrato = 1\nnome = \"X\"\ncriar_diretorios = [\"{dir}\"]\n[[pkcs11]]\nmodulo = \"a.so\"\n"
            );
            assert!(
                matches!(
                    driver(&texto),
                    Err(Erro::Caminho {
                        campo: "criar_diretorios",
                        ..
                    })
                ),
                "{dir:?} deveria ser recusado"
            );
        }
    }

    #[test]
    fn campo_desconhecido_e_erro_e_nao_silencio() {
        let r = driver("contrato = 1\nnome = \"X\"\n[[pkcs11]]\nmodlo = \"a.so\"\n");
        assert!(matches!(r, Err(Erro::Formato(_))), "{r:?}");
    }

    #[test]
    fn contrato_mais_novo_pede_atualizar_mesmo_com_campo_desconhecido() {
        let r = driver("contrato = 2\nnome = \"X\"\ncampo_do_futuro = true\n");
        assert_eq!(r, Err(Erro::ContratoNovo(2)));
    }

    #[test]
    fn sem_contrato_e_erro_de_formato() {
        assert!(matches!(
            driver("nome = \"X\"\n[[pkcs11]]\nmodulo = \"a.so\"\n"),
            Err(Erro::Formato(_))
        ));
    }

    #[test]
    fn cada_familia_exige_o_que_ela_oferece() {
        let so_ferramenta =
            "contrato = 1\nnome = \"X\"\n[[ferramenta]]\nnome = \"X\"\nexecutavel = \"bin/x\"\n";
        let dir = Path::new(DIR);
        let home = Path::new(HOME);
        assert_eq!(
            interpretar(so_ferramenta, dir, Familia::Driver, home),
            Err(Erro::Vazia(Familia::Driver))
        );
        assert_eq!(
            interpretar(so_ferramenta, dir, Familia::Assinador, home),
            Err(Erro::Vazia(Familia::Assinador))
        );
        assert!(interpretar(so_ferramenta, dir, Familia::App, home).is_ok());
    }

    #[test]
    fn um_assinador_aponta_os_manifestos_do_fabricante() {
        let d = interpretar(
            r#"
            contrato = 1
            nome = "Web PKI"
            fabricante = "Lacuna Software"
            [[assinador]]
            host = "com.lacunasoftware.webpki"
            executavel = "extra/bin/webpki"
            manifesto_firefox = "extra/native-messaging/firefox.json"
            manifesto_chromium = "extra/native-messaging/chromium.json"
        "#,
            Path::new("/app/extensoes/assinador/WebPKI"),
            Familia::Assinador,
            Path::new(HOME),
        )
        .unwrap();
        let a = &d.assinadores[0];
        assert_eq!(a.host, "com.lacunasoftware.webpki");
        assert_eq!(
            a.manifesto_firefox,
            Some(PathBuf::from(
                "/app/extensoes/assinador/WebPKI/extra/native-messaging/firefox.json"
            ))
        );
    }

    /// Uma raiz de extensões num diretório temporário, apagada ao sair.
    struct Raiz(PathBuf);

    impl Raiz {
        fn nova(nome: &str) -> Raiz {
            let p = std::env::temp_dir().join(format!("ancora-{}-{nome}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Raiz(p)
        }

        fn extensao(&self, familia: Familia, id: &str, descritor: Option<&str>) {
            let dir = self.0.join(familia.diretorio()).join(id);
            fs::create_dir_all(&dir).unwrap();
            if let Some(texto) = descritor {
                fs::write(dir.join(ARQUIVO), texto).unwrap();
            }
        }
    }

    impl Drop for Raiz {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn encontra_por_familia_e_mantem_a_quebrada_na_lista() {
        let raiz = Raiz::nova("encontrar");
        raiz.extensao(Familia::Driver, "SafeSign", Some(SAFESIGN));
        raiz.extensao(
            Familia::Driver,
            "Quebrado",
            Some("contrato = 1\nnome = 3\n"),
        );
        raiz.extensao(Familia::Driver, "SemDescritor", None);
        raiz.extensao(
            Familia::App,
            "PJeOffice",
            Some("contrato = 1\nnome = \"PJeOffice Pro\"\n[[ferramenta]]\nnome = \"PJeOffice Pro\"\nexecutavel = \"bin/pjeoffice\"\natalho = true\n"),
        );

        let achadas = encontrar(&raiz.0, Path::new(HOME));
        let resumo: Vec<_> = achadas
            .iter()
            .map(|e| (e.familia, e.id.as_str(), e.descritor.is_ok()))
            .collect();
        assert_eq!(
            resumo,
            vec![
                (Familia::Driver, "Quebrado", false),
                (Familia::Driver, "SafeSign", true),
                (Familia::App, "PJeOffice", true),
            ]
        );
        assert_eq!(achadas[1].diretorio, raiz.0.join("driver/SafeSign"));
        assert!(achadas[2].descritor.as_ref().unwrap().ferramentas[0].atalho);
    }

    #[test]
    fn sem_raiz_nao_ha_extensoes() {
        assert!(encontrar(Path::new("/nao/existe"), Path::new(HOME)).is_empty());
    }
}
