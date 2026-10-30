//! Garantir que cada consumidor veja cada token exatamente uma vez.
//!
//! Em vez de confiar numa receita por distribuição, o Âncora mede: publica por
//! alguns segundos o token-sentinela (`sentinela/sentinela.c`), abre o banco
//! NSS do consumidor do jeito que o consumidor abre, dentro do ambiente dele
//! (o sandbox de um aplicativo Flatpak, ou o host), e conta quantas vezes o
//! sentinela aparece. É quantas vezes um token de verdade apareceria. Depois
//! tenta as formas de registro em ordem e fica com a primeira que dá 1.

use crate::{processo, publicacao};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

/// O rótulo do token-sentinela.
pub const ROTULO: &str = "Âncora: verificação";

/// Onde o manifesto instala o sentinela.
const BIBLIOTECA: &str = "/app/lib/ancora/libancora-sentinela.so";

/// A sonda que roda do lado do consumidor, passada pela entrada padrão.
const SONDA: &str = include_str!("sonda_nss.py");

/// Abrir um sandbox Flatpak leva alguns segundos na primeira vez.
const LIMITE: Duration = Duration::from_secs(60);

/// Aponta a sonda para outro servidor do sistema. Só em testes: o servidor
/// da sessão serviria os tokens reais.
pub const VARIAVEL_SERVIDOR: &str = "ANCORA_SERVIDOR_DO_SISTEMA";

/// Como o consumidor abre o banco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Raizes {
    /// Firefox e poppler: o NSS carrega sozinho o módulo de raízes.
    Automaticas,
    /// O Chromium desliga isso e carrega o `libnssckbi.so` ele mesmo.
    Chromium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ambiente<'a> {
    Host,
    Flatpak {
        app: &'a str,
        /// O diretório da `libnss3` do próprio aplicativo, quando ele traz a
        /// dele (o Firefox, em `/app/lib/firefox`).
        nss_proprio: Option<&'a str>,
    },
}

/// Quantas vezes o sentinela aparece, e por quais módulos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Contagem {
    pub vezes: usize,
    pub modulos: Vec<String>,
}

pub trait Medidor {
    fn contar(&self, ambiente: Ambiente, banco: &Path, raizes: Raizes) -> Result<Contagem, String>;
}

impl<F> Medidor for F
where
    F: Fn(Ambiente, &Path, Raizes) -> Result<Contagem, String>,
{
    fn contar(&self, ambiente: Ambiente, banco: &Path, raizes: Raizes) -> Result<Contagem, String> {
        self(ambiente, banco, raizes)
    }
}

/// Um medidor que não pode medir, e diz por quê.
pub struct Indisponivel(pub String);

impl Medidor for Indisponivel {
    fn contar(&self, _: Ambiente, _: &Path, _: Raizes) -> Result<Contagem, String> {
        Err(self.0.clone())
    }
}

/// O sentinela publicado. Despublica e para o servidor ao sair de cena.
pub struct Sentinela {
    servidor: Child,
    modulo: PathBuf,
    home_host: PathBuf,
}

impl Sentinela {
    pub fn publicar(home_host: &Path) -> Result<Sentinela, String> {
        if !Path::new(BIBLIOTECA).is_file() {
            return Err("o token-sentinela só existe dentro do Flatpak do Âncora".into());
        }
        let dir = gtk::glib::user_runtime_dir().join("ancora");
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let socket = dir.join("verificacao");
        let _ = fs::remove_file(&socket);
        let servidor = processo::comando_servidor(&socket, Path::new(BIBLIOTECA))
            .spawn()
            .map_err(|e| format!("p11-kit server: {e}"))?;
        let sentinela = Sentinela {
            servidor,
            modulo: publicacao::diretorio(home_host).join(publicacao::SENTINELA),
            home_host: home_host.to_path_buf(),
        };
        let limite = Instant::now() + Duration::from_secs(5);
        while !socket.exists() {
            if Instant::now() >= limite {
                return Err("o servidor do sentinela não subiu".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let dir_modules = publicacao::diretorio(home_host);
        fs::create_dir_all(&dir_modules).map_err(|e| format!("{}: {e}", dir_modules.display()))?;
        fs::write(&sentinela.modulo, publicacao::conteudo_sentinela(&socket))
            .map_err(|e| format!("{}: {e}", sentinela.modulo.display()))?;
        Ok(sentinela)
    }
}

impl Drop for Sentinela {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.modulo);
        processo::parar_grupo(&mut self.servidor);
    }
}

impl Medidor for Sentinela {
    fn contar(&self, ambiente: Ambiente, banco: &Path, raizes: Raizes) -> Result<Contagem, String> {
        let lista = listar(ambiente, banco, raizes, &self.home_host)?;
        Ok(contar_sentinela(&lista))
    }
}

pub fn contar_sentinela(lista: &[(String, String)]) -> Contagem {
    let modulos: Vec<String> = lista
        .iter()
        .filter(|(_, token)| token == ROTULO)
        .map(|(modulo, _)| modulo.clone())
        .collect();
    Contagem {
        vezes: modulos.len(),
        modulos,
    }
}

fn raizes_arg(r: Raizes) -> &'static str {
    match r {
        Raizes::Automaticas => "auto",
        Raizes::Chromium => "chromium",
    }
}

/// Os tokens (com o módulo de cada um) que o consumidor enxerga no banco.
fn listar(
    ambiente: Ambiente,
    banco: &Path,
    raizes: Raizes,
    home_host: &Path,
) -> Result<Vec<(String, String)>, String> {
    let servidor = std::env::var_os(VARIAVEL_SERVIDOR).map(PathBuf::from);
    let mut argumentos: Vec<String> = vec![
        "-".into(),
        "libnss3.so".into(),
        banco.display().to_string(),
        raizes_arg(raizes).into(),
    ];
    if let Some(s) = &servidor {
        argumentos.push(s.display().to_string());
    }

    let rodar = |cmd: Command| -> Result<std::process::Output, String> {
        processo::executar(cmd, SONDA.as_bytes(), LIMITE)
    };
    let saida = match ambiente {
        Ambiente::Host => {
            // O Python do sistema, não um que o usuário tenha posto antes no
            // PATH; o do PATH só se o do sistema não existir.
            let tentar = |python: &str| {
                let mut cmd = Command::new("flatpak-spawn");
                cmd.arg("--host")
                    .arg(format!("--env=HOME={}", home_host.display()))
                    .arg(format!(
                        "--env=XDG_CONFIG_HOME={}",
                        home_host.join(".config").display()
                    ))
                    .arg(python)
                    .args(&argumentos);
                rodar(cmd)
            };
            match tentar("/usr/bin/python3")? {
                s if s.status.code() == Some(127) => tentar("python3")?,
                s => s,
            }
        }
        Ambiente::Flatpak { app, nss_proprio } => {
            let mut cmd = Command::new("flatpak-spawn");
            cmd.args(["--host", "flatpak", "run", "--command=python3"]);
            if let Some(dir) = nss_proprio {
                cmd.arg(format!("--env=LD_LIBRARY_PATH={dir}"));
            }
            // Nos testes o banco e o servidor ficam fora dos diretórios do
            // aplicativo.
            if std::env::var_os(crate::host::VARIAVEL).is_some() {
                cmd.arg(format!("--filesystem={}", banco.display()));
            }
            if let Some(s) = servidor.as_ref().and_then(|s| s.parent()) {
                cmd.arg(format!("--filesystem={}", s.display()));
            }
            cmd.arg(app).args(&argumentos);
            rodar(cmd)?
        }
    };
    if !saida.status.success() {
        return Err(format!(
            "a sonda falhou: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        ));
    }
    let texto = String::from_utf8_lossy(&saida.stdout);
    let linha = texto.lines().last().unwrap_or_default();
    serde_json::from_str::<Vec<(String, String)>>(linha)
        .map_err(|_| format!("resposta inesperada da sonda: {linha}"))
}

/// O que ficou registrado e como se chegou lá.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resultado {
    /// O consumidor vê o token uma vez. `None`: sem registro do Âncora.
    Uma(Option<String>),
    /// Não deu para medir; ficou o registro padrão.
    NaoVerificado {
        registro: Option<String>,
        motivo: String,
    },
}

/// Tenta `opcoes` em ordem (`None` é não registrar nada) e fica com a
/// primeira em que o sentinela aparece exatamente uma vez. Se não der para
/// medir, aplica `padrao` e diz por quê.
pub fn garantir_um(
    opcoes: &[Option<String>],
    padrao: &Option<String>,
    mut aplicar: impl FnMut(&Option<String>) -> Result<(), String>,
    mut contar: impl FnMut() -> Result<Contagem, String>,
) -> Result<Resultado, String> {
    let mut tentativas = Vec::new();
    for opcao in opcoes {
        aplicar(opcao)?;
        let contagem = match contar() {
            Ok(c) => c,
            Err(motivo) => {
                aplicar(padrao)?;
                return Ok(Resultado::NaoVerificado {
                    registro: padrao.clone(),
                    motivo,
                });
            }
        };
        if contagem.vezes == 1 {
            return Ok(Resultado::Uma(opcao.clone()));
        }
        if opcao.is_none() && contagem.vezes > 1 {
            // Já duplica sem o Âncora: nenhum registro dele conserta isso.
            return Err(format!(
                "o token aparece {} vezes mesmo sem o Âncora, por: {}",
                contagem.vezes,
                contagem.modulos.join(", ")
            ));
        }
        let nome = opcao.as_deref().unwrap_or("sem registro");
        tentativas.push(format!("{nome}: {} vezes", contagem.vezes));
    }
    aplicar(&None)?;
    Err(format!(
        "nenhuma forma de registro deixou o token uma vez só ({})",
        tentativas.join("; ")
    ))
}

/// Aplica `garantir_um` a um banco NSS: cada opção é tirar o bloco do Âncora
/// (`None`) ou registrá-lo com aquela biblioteca.
pub fn garantir_um_no_banco(
    banco: &Path,
    nome: &str,
    medidor: &dyn Medidor,
    ambiente: Ambiente,
    raizes: Raizes,
    opcoes: &[Option<String>],
    padrao: &Option<String>,
) -> Result<Resultado, String> {
    garantir_um(
        opcoes,
        padrao,
        |opcao| {
            let r = match opcao {
                None => crate::nss::editar(banco, |t| crate::nss::sem_modulo(t, nome)),
                Some(lib) => crate::nss::editar(banco, |t| crate::nss::com_modulo(t, nome, lib)),
            };
            r.map(|_| ())
                .map_err(|e| format!("{}: {e}", banco.display()))
        },
        || medidor.contar(ambiente, banco, raizes),
    )
    .map_err(|e| format!("{}: {e}", banco.display()))
}

/// O que foi decidido para um banco, guardado para saber depois se ele
/// continua como foi verificado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Registro {
    pub biblioteca: Option<String>,
    pub verificado: bool,
}

impl Registro {
    pub fn de(resultado: &Resultado) -> Registro {
        match resultado {
            Resultado::Uma(b) => Registro {
                biblioteca: b.clone(),
                verificado: true,
            },
            Resultado::NaoVerificado { registro, .. } => Registro {
                biblioteca: registro.clone(),
                verificado: false,
            },
        }
    }

    /// O banco continua como foi decidido: o bloco `nome` tem a mesma
    /// biblioteca, ou continua ausente.
    pub fn vale_para(&self, banco: &Path, nome: &str) -> bool {
        crate::nss::biblioteca_do_ancora(&crate::nss::ler(banco), nome) == self.biblioteca
    }
}

fn arquivo_registros(dados: &Path) -> PathBuf {
    dados.join("verificados")
}

/// Cada registro é de um bloco num banco: o mesmo `~/.pki/nssdb` tem o dos
/// navegadores nativos e o do Chromium Flatpak.
pub type Chave = (PathBuf, String);

/// Os registros, um por linha: banco, bloco, biblioteca (`-` para nenhuma) e
/// `verificado` ou `nao-verificado`, separados por tabulação.
pub fn ler_registros(dados: &Path) -> std::collections::BTreeMap<Chave, Registro> {
    fs::read_to_string(arquivo_registros(dados))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut c = l.split('\t');
            let banco = PathBuf::from(c.next()?);
            let nome = c.next()?.to_owned();
            let biblioteca = match c.next()? {
                "-" => None,
                b => Some(b.to_owned()),
            };
            let verificado = c.next()? == "verificado";
            Some((
                (banco, nome),
                Registro {
                    biblioteca,
                    verificado,
                },
            ))
        })
        .collect()
}

/// Guarda (ou, com `None`, esquece) o registro de um banco.
pub fn gravar_registro(
    dados: &Path,
    banco: &Path,
    nome: &str,
    registro: Option<Registro>,
) -> Result<(), String> {
    let mut todos = ler_registros(dados);
    let chave = (banco.to_path_buf(), nome.to_owned());
    match registro {
        Some(r) => {
            todos.insert(chave, r);
        }
        None => {
            todos.remove(&chave);
        }
    }
    let texto: String = todos
        .iter()
        .map(|((b, nome), r)| {
            format!(
                "{}\t{nome}\t{}\t{}\n",
                b.display(),
                r.biblioteca.as_deref().unwrap_or("-"),
                if r.verificado {
                    "verificado"
                } else {
                    "nao-verificado"
                }
            )
        })
        .collect();
    fs::create_dir_all(dados).map_err(|e| format!("{}: {e}", dados.display()))?;
    fs::write(arquivo_registros(dados), texto).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Simula um consumidor: quantas vezes o sentinela aparece para cada
    /// estado do registro.
    fn simular(
        vezes: impl Fn(&Option<String>) -> usize,
        opcoes: &[Option<String>],
        padrao: Option<String>,
    ) -> (Result<Resultado, String>, Option<String>) {
        let estado = RefCell::new(Some("lixo".to_owned()));
        let r = garantir_um(
            opcoes,
            &padrao,
            |o| {
                *estado.borrow_mut() = o.clone();
                Ok(())
            },
            || {
                Ok(Contagem {
                    vezes: vezes(&estado.borrow()),
                    modulos: vec!["X".into(); vezes(&estado.borrow())],
                })
            },
        );
        (r, estado.into_inner())
    }

    fn s(x: &str) -> Option<String> {
        Some(x.to_owned())
    }

    #[test]
    fn se_ja_aparece_uma_vez_nao_registra() {
        // O Chromium Flatpak, ou um sistema que já põe o proxy no NSS.
        let (r, final_) = simular(
            |o| if o.is_none() { 1 } else { 2 },
            &[None, s("cliente")],
            None,
        );
        assert_eq!(r, Ok(Resultado::Uma(None)));
        assert_eq!(final_, None);
    }

    #[test]
    fn registra_a_primeira_forma_que_da_um() {
        // Debian: o caminho absoluto do host carrega também no sandbox (2), o
        // nome curto não.
        let (r, final_) = simular(
            |o| match o.as_deref() {
                None => 0,
                Some("/abs") => 2,
                Some(_) => 1,
            },
            &[None, s("/abs"), s("curto")],
            s("/abs"),
        );
        assert_eq!(r, Ok(Resultado::Uma(s("curto"))));
        assert_eq!(final_, s("curto"));
    }

    #[test]
    fn duplicado_sem_o_ancora_e_erro_e_nao_deixa_registro() {
        let (r, final_) = simular(|_| 2, &[None, s("cliente")], None);
        let erro = r.unwrap_err();
        assert!(erro.contains("2 vezes mesmo sem o Âncora"), "{erro}");
        assert_eq!(final_, None);
    }

    #[test]
    fn nenhuma_forma_serve_e_erro_com_as_tentativas() {
        let (r, final_) = simular(|_| 0, &[None, s("cliente")], s("cliente"));
        let erro = r.unwrap_err();
        assert!(erro.contains("cliente: 0 vezes"), "{erro}");
        assert_eq!(final_, None);
    }

    #[test]
    fn sem_poder_medir_fica_o_padrao() {
        let estado = RefCell::new(None);
        let r = garantir_um(
            &[None, s("cliente")],
            &s("cliente"),
            |o| {
                *estado.borrow_mut() = o.clone();
                Ok(())
            },
            || Err("sem python3".into()),
        );
        assert_eq!(
            r,
            Ok(Resultado::NaoVerificado {
                registro: s("cliente"),
                motivo: "sem python3".into()
            })
        );
        assert_eq!(estado.into_inner(), s("cliente"));
    }

    #[test]
    fn registros_vao_e_voltam_e_conferem_o_banco() {
        use crate::nss::{NOME, NOME_NATIVO};
        let d = std::env::temp_dir().join(format!("ancora-reg-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let banco = d.join("banco");
        fs::create_dir_all(&banco).unwrap();
        fs::write(banco.join("pkcs11.txt"), "").unwrap();
        let chave = |nome: &str| (banco.clone(), nome.to_owned());

        // O mesmo banco com um registro por bloco: nativo e Flatpak.
        let nativo = Registro::de(&Resultado::Uma(s("/usr/lib/p11-kit-proxy.so")));
        let flatpak = Registro::de(&Resultado::Uma(None));
        gravar_registro(&d, &banco, NOME_NATIVO, Some(nativo.clone())).unwrap();
        gravar_registro(&d, &banco, NOME, Some(flatpak.clone())).unwrap();
        assert_eq!(ler_registros(&d).get(&chave(NOME_NATIVO)), Some(&nativo));
        assert_eq!(ler_registros(&d).get(&chave(NOME)), Some(&flatpak));

        assert!(
            !nativo.vale_para(&banco, NOME_NATIVO),
            "o bloco ainda não está no banco"
        );
        assert!(flatpak.vale_para(&banco, NOME));
        crate::nss::editar(&banco, |t| {
            crate::nss::com_modulo(t, NOME_NATIVO, "/usr/lib/p11-kit-proxy.so")
        })
        .unwrap();
        assert!(nativo.vale_para(&banco, NOME_NATIVO));
        assert!(
            flatpak.vale_para(&banco, NOME),
            "o bloco do outro dono não conta"
        );

        gravar_registro(&d, &banco, NOME_NATIVO, None).unwrap();
        assert_eq!(ler_registros(&d).get(&chave(NOME_NATIVO)), None);
        assert_eq!(ler_registros(&d).get(&chave(NOME)), Some(&flatpak));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn conta_so_o_sentinela_e_diz_por_onde() {
        let lista = vec![
            (
                "NSS Internal PKCS #11 Module".into(),
                "NSS Certificate DB".into(),
            ),
            ("Âncora (p11-kit)".into(), ROTULO.into()),
            ("Root Certs".into(), ROTULO.into()),
            ("Âncora (p11-kit)".into(), "HUBTESTE".into()),
        ];
        assert_eq!(
            contar_sentinela(&lista),
            Contagem {
                vezes: 2,
                modulos: vec!["Âncora (p11-kit)".into(), "Root Certs".into()]
            }
        );
    }
}
