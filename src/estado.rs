//! O que passa entre o serviço e a janela, que são processos à parte (e
//! podem estar em instâncias diferentes do sandbox, se o serviço veio do
//! início automático da sessão).
//!
//! - o serviço escreve o que cada driver está fazendo em
//!   `$XDG_RUNTIME_DIR/ancora/estado.json`, a cada mudança;
//! - a janela sabe se o serviço está vivo pela trava dele, e não pelo arquivo,
//!   que sobra de um SIGKILL;
//! - a janela desliga drivers escrevendo a lista nos dados do Âncora, e pede
//!   ao serviço para parar criando `parar` ao lado da trava.
//!
//! O serviço confere os dois pedidos a cada passo: são arquivos pequenos, e um
//! `stat` a cada 200 ms não pesa.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

const ARQUIVO: &str = "estado.json";
const TRAVA: &str = "servico.trava";
const PARAR: &str = "parar";
const DESLIGADOS: &str = "drivers-desligados";

/// Onde ficam os sockets dos drivers, a trava e o estado do serviço.
pub fn diretorio() -> PathBuf {
    gtk::glib::user_runtime_dir().join("ancora")
}

/// Redireciona os dados do Âncora, para testes. O `XDG_DATA_HOME` não serve:
/// o Flatpak o sobrescreve, e a permissão `xdg-data/ancora` monta a pasta do
/// host por cima da do aplicativo.
pub const VARIAVEL_DADOS: &str = "ANCORA_DADOS";

/// Os dados do Âncora: registros do que ele concedeu, verificou e publicou, e
/// as escolhas da pessoa. No sandbox, é a `~/.local/share/ancora` do host.
pub fn dados() -> PathBuf {
    match std::env::var_os(VARIAVEL_DADOS) {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => gtk::glib::user_data_dir().join("ancora"),
    }
}

pub fn caminho_trava(dir: &Path) -> PathBuf {
    dir.join(TRAVA)
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Estado {
    pub drivers: Vec<Driver>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Driver {
    /// O id da extensão (`SafeSign`), o mesmo para todos os módulos dela.
    pub extensao: String,
    /// `SafeSign`, ou `SafeSign/2` no segundo módulo.
    pub rotulo: String,
    pub socket: PathBuf,
    pub situacao: Situacao,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "kebab-case")]
pub enum Situacao {
    Iniciando,
    Publicado { tokens: Vec<String> },
    ComErro { motivo: String },
    Parado { motivo: String },
    Desligado,
}

/// Escreve o estado de uma vez: quem lê nunca vê um arquivo pela metade.
pub fn gravar(dir: &Path, estado: &Estado) -> io::Result<()> {
    let temporario = dir.join(format!(".{ARQUIVO}"));
    let texto = serde_json::to_string(estado).map_err(io::Error::other)?;
    fs::write(&temporario, texto)?;
    fs::rename(temporario, dir.join(ARQUIVO))
}

pub fn apagar(dir: &Path) {
    let _ = fs::remove_file(dir.join(ARQUIVO));
}

/// O estado do serviço, se ele está rodando.
pub fn ler(dir: &Path) -> Option<Estado> {
    if !rodando(dir) {
        return None;
    }
    // Rodando, mas sem estado ainda: está subindo.
    let texto = fs::read_to_string(dir.join(ARQUIVO)).unwrap_or_default();
    Some(serde_json::from_str(&texto).unwrap_or_default())
}

/// Se há um serviço segurando a trava.
pub fn rodando(dir: &Path) -> bool {
    let Ok(arquivo) = File::open(caminho_trava(dir)) else {
        return false;
    };
    // Conseguir a trava, mesmo compartilhada, quer dizer que ninguém segura a
    // exclusiva. Ela é solta ao fechar o arquivo, no fim desta função.
    matches!(arquivo.try_lock_shared(), Err(fs::TryLockError::WouldBlock))
}

/// Pede ao serviço que pare.
pub fn pedir_parada(dir: &Path) -> io::Result<()> {
    fs::write(dir.join(PARAR), "")
}

/// Se alguém pediu ao serviço que pare. Consome o pedido.
pub fn parada_pedida(dir: &Path) -> bool {
    fs::remove_file(dir.join(PARAR)).is_ok()
}

/// Apaga um pedido de parada que sobrou de um serviço que já tinha saído.
pub fn descartar_parada(dir: &Path) {
    let _ = fs::remove_file(dir.join(PARAR));
}

pub fn caminho_desligados(dados: &Path) -> PathBuf {
    dados.join(DESLIGADOS)
}

/// As extensões de driver que a pessoa desligou.
pub fn desligados(dados: &Path) -> BTreeSet<String> {
    ler_lista(&caminho_desligados(dados))
}

pub fn ligar(dados: &Path, extensao: &str, ligado: bool) -> io::Result<()> {
    let mut lista = desligados(dados);
    if ligado {
        lista.remove(extensao);
    } else {
        lista.insert(extensao.to_owned());
    }
    gravar_lista(&caminho_desligados(dados), &lista)
}

/// Um conjunto de nomes, um por linha.
pub fn ler_lista(arquivo: &Path) -> BTreeSet<String> {
    fs::read_to_string(arquivo)
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn gravar_lista(arquivo: &Path, lista: &BTreeSet<String>) -> io::Result<()> {
    if let Some(pai) = arquivo.parent() {
        fs::create_dir_all(pai)?;
    }
    let texto: String = lista.iter().map(|n| format!("{n}\n")).collect();
    fs::write(arquivo, texto)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn dir(nome: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("ancora-estado-{}-{nome}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    #[test]
    fn sem_trava_segura_o_servico_nao_esta_rodando() {
        let d = dir("trava");
        assert!(!rodando(&d.0));
        // Uma trava que sobrou de um serviço morto não conta.
        File::create(caminho_trava(&d.0)).unwrap();
        assert!(!rodando(&d.0));
        gravar(&d.0, &Estado::default()).unwrap();
        assert_eq!(ler(&d.0), None);

        let trava = File::create(caminho_trava(&d.0)).unwrap();
        trava.try_lock().unwrap();
        assert!(rodando(&d.0));
        // Conferir não pode soltar a trava de quem a segura.
        assert!(rodando(&d.0));
        drop(trava);
        assert!(!rodando(&d.0));
    }

    #[test]
    fn o_estado_volta_como_foi_gravado() {
        let d = dir("ida-e-volta");
        let trava = File::create(caminho_trava(&d.0)).unwrap();
        trava.try_lock().unwrap();
        // Rodando e ainda sem estado: subindo, sem drivers.
        assert_eq!(ler(&d.0), Some(Estado::default()));
        let estado = Estado {
            drivers: vec![
                Driver {
                    extensao: "SafeSign".into(),
                    rotulo: "SafeSign".into(),
                    socket: d.0.join("safesign"),
                    situacao: Situacao::Publicado {
                        tokens: vec!["GD Burti".into()],
                    },
                },
                Driver {
                    extensao: "SafeNet".into(),
                    rotulo: "SafeNet".into(),
                    socket: d.0.join("safenet"),
                    situacao: Situacao::Parado {
                        motivo: "o driver não respondeu 3 vezes seguidas".into(),
                    },
                },
            ],
        };
        gravar(&d.0, &estado).unwrap();
        assert_eq!(ler(&d.0), Some(estado));
        apagar(&d.0);
        assert_eq!(ler(&d.0), Some(Estado::default()));
    }

    #[test]
    fn o_pedido_de_parada_vale_uma_vez() {
        let d = dir("parada");
        assert!(!parada_pedida(&d.0));
        pedir_parada(&d.0).unwrap();
        assert!(parada_pedida(&d.0));
        assert!(!parada_pedida(&d.0));
    }

    #[test]
    fn desligar_e_ligar_um_driver() {
        let d = dir("desligados");
        let dados = d.0.join("dados");
        assert!(desligados(&dados).is_empty());
        ligar(&dados, "SafeNet", false).unwrap();
        ligar(&dados, "SafeSign", false).unwrap();
        ligar(&dados, "SafeNet", false).unwrap();
        assert_eq!(
            desligados(&dados),
            BTreeSet::from(["SafeNet".to_owned(), "SafeSign".to_owned()])
        );
        ligar(&dados, "SafeNet", true).unwrap();
        assert_eq!(desligados(&dados), BTreeSet::from(["SafeSign".to_owned()]));
    }
}
