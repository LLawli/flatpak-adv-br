//! `ancora servir`: o serviço que publica os drivers.
//!
//! Para cada módulo PKCS#11 das extensões de driver, sobe um `p11-kit server`
//! em `$XDG_RUNTIME_DIR/ancora/<chave>`, sonda-o periodicamente num processo
//! à parte e mantém publicado no host (ver `publicacao`) só o que respondeu
//! da última vez. Ao receber SIGTERM, SIGINT ou SIGHUP, despublica tudo e
//! para os servidores.
//!
//! As extensões são lidas uma vez, na partida: o Flatpak monta as extensões
//! quando o sandbox começa, e uma instalada depois só aparece num serviço
//! reiniciado.

use crate::extensao::{self, Familia};
use crate::supervisao::{Acao, Estado, Sondagem, Supervisao};
use crate::{processo, publicacao};
use gtk::glib;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// De quanto em quanto tempo cada driver é sondado.
const INTERVALO: Duration = Duration::from_secs(10);
/// Quanto uma sonda pode levar antes de o driver contar como travado.
const LIMITE_SONDA: Duration = Duration::from_secs(10);
/// A primeira sonda depois de (re)iniciar um servidor, para dar tempo ao socket.
const ESPERA_INICIAL: Duration = Duration::from_secs(1);
/// O passo do laço que acompanha servidores e sondas.
const PASSO: Duration = Duration::from_millis(200);

struct Driver {
    /// Como aparece no log: `SafeSign`, ou `SafeSign/2` no segundo módulo.
    rotulo: String,
    modulo: extensao::Pkcs11,
    criar_diretorios: Vec<PathBuf>,
    socket: PathBuf,
    servidor: Option<Child>,
    sonda: Option<(Child, Instant)>,
    proxima_sonda: Instant,
    supervisao: Supervisao,
}

struct Servico {
    dir_modules: PathBuf,
    drivers: BTreeMap<String, Driver>,
    /// Mantém a trava de instância única enquanto o serviço existe.
    _trava: File,
}

pub fn servir() -> glib::ExitCode {
    match iniciar() {
        Ok(servico) => rodar(servico),
        Err(e) => {
            eprintln!("ancora servir: {e}");
            glib::ExitCode::FAILURE
        }
    }
}

fn iniciar() -> Result<Servico, String> {
    let home = glib::home_dir();
    let dir_sockets = glib::user_runtime_dir().join("ancora");
    fs::create_dir_all(&dir_sockets).map_err(|e| format!("{}: {e}", dir_sockets.display()))?;

    let caminho_trava = dir_sockets.join("servico.trava");
    let trava =
        File::create(&caminho_trava).map_err(|e| format!("{}: {e}", caminho_trava.display()))?;
    trava
        .try_lock()
        .map_err(|_| "já há um serviço do Âncora rodando".to_owned())?;

    // Uma queda anterior pode ter deixado módulos publicados. Apontam para
    // sockets mortos, o que não trava ninguém, mas não devem ficar.
    let dir_modules = publicacao::diretorio(&crate::host::home());
    publicacao::publicar(&dir_modules, &BTreeMap::new())
        .map_err(|e| format!("{}: {e}", dir_modules.display()))?;

    let mut drivers = BTreeMap::new();
    for e in extensao::encontrar(Path::new(extensao::RAIZ), &home) {
        if e.familia != Familia::Driver {
            continue;
        }
        let d = match e.descritor {
            Ok(d) => d,
            Err(erro) => {
                eprintln!("{}: {erro}", e.id);
                continue;
            }
        };
        let varios = d.pkcs11.len() > 1;
        for (i, modulo) in d.pkcs11.into_iter().enumerate() {
            let (chave, rotulo) = if varios {
                (
                    format!("{}-{}", e.id.to_lowercase(), i + 1),
                    format!("{}/{}", e.id, i + 1),
                )
            } else {
                (e.id.to_lowercase(), e.id.clone())
            };
            if publicacao::nome(&chave) == publicacao::SENTINELA {
                eprintln!("{rotulo}: o nome é reservado ao token-sentinela");
                continue;
            }
            drivers.insert(
                chave.clone(),
                Driver {
                    rotulo,
                    modulo,
                    criar_diretorios: d.criar_diretorios.clone(),
                    socket: dir_sockets.join(&chave),
                    servidor: None,
                    sonda: None,
                    proxima_sonda: Instant::now(),
                    supervisao: Supervisao::default(),
                },
            );
        }
    }
    if drivers.is_empty() {
        eprintln!("nenhum driver instalado");
    }
    Ok(Servico {
        dir_modules,
        drivers,
        _trava: trava,
    })
}

fn rodar(mut servico: Servico) -> glib::ExitCode {
    for d in servico.drivers.values_mut() {
        iniciar_servidor(d);
    }
    let servico = Rc::new(RefCell::new(servico));
    let laco = glib::MainLoop::new(None, false);

    for sinal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
        let servico = servico.clone();
        let laco = laco.clone();
        glib_unix::unix_signal_add_local(sinal, move || {
            encerrar(&mut servico.borrow_mut());
            laco.quit();
            glib::ControlFlow::Break
        });
    }

    {
        let servico = servico.clone();
        glib::timeout_add_local(PASSO, move || {
            acompanhar(&mut servico.borrow_mut());
            glib::ControlFlow::Continue
        });
    }

    laco.run();
    glib::ExitCode::SUCCESS
}

fn acompanhar(servico: &mut Servico) {
    let mut mudou = false;
    for d in servico.drivers.values_mut() {
        if d.supervisao.parado() {
            continue;
        }
        let antes = d.supervisao.estado.clone();
        if let Some(sondagem) = observar(d) {
            match d.supervisao.registrar(sondagem) {
                Acao::Nenhuma => {}
                Acao::Reiniciar => {
                    parar_servidor(d);
                    iniciar_servidor(d);
                }
                Acao::Parar => parar_servidor(d),
            }
        }
        if d.supervisao.estado != antes {
            eprintln!("{}: {}", d.rotulo, descrever(&d.supervisao.estado));
            mudou = true;
        }
    }
    if mudou {
        publicar(servico);
    }
}

/// O que aconteceu com o driver desde o último passo, se algo terminou.
fn observar(d: &mut Driver) -> Option<Sondagem> {
    if let Some(servidor) = &mut d.servidor
        && let Ok(Some(_)) = servidor.try_wait()
    {
        d.servidor = None;
        matar_sonda(d);
        return Some(Sondagem::Caiu);
    }
    if let Some((sonda, inicio)) = &mut d.sonda {
        return match sonda.try_wait() {
            Ok(Some(_)) => {
                let (sonda, _) = d.sonda.take().expect("sonda em andamento");
                Some(resultado(sonda))
            }
            Ok(None) if inicio.elapsed() > LIMITE_SONDA => {
                // Contado antes de matar a sonda: uma das conexões é a dela.
                let ocupado = d.servidor.as_ref().is_some_and(|s| conexoes(s.id()) > 1);
                matar_sonda(d);
                Some(if ocupado {
                    Sondagem::Ocupado
                } else {
                    Sondagem::Travou
                })
            }
            Ok(None) => None,
            Err(e) => {
                d.sonda = None;
                Some(Sondagem::Falhou(e.to_string()))
            }
        };
    }
    if Instant::now() >= d.proxima_sonda {
        d.proxima_sonda = Instant::now() + INTERVALO;
        match iniciar_sonda(&d.socket) {
            Ok(sonda) => d.sonda = Some((sonda, Instant::now())),
            Err(e) => return Some(Sondagem::Falhou(e)),
        }
    }
    None
}

/// Quantas conexões o servidor atende agora: o `p11-kit server` abre um
/// processo filho por conexão.
fn conexoes(servidor: u32) -> usize {
    let Ok(entradas) = fs::read_dir("/proc") else {
        return 0;
    };
    entradas
        .flatten()
        .filter_map(|e| fs::read_to_string(e.path().join("stat")).ok())
        .filter(|stat| pai(stat) == Some(servidor))
        .count()
}

/// O PPID de uma linha de `/proc/<pid>/stat`. O nome do processo vem entre
/// parênteses e pode ter espaços: os campos contam a partir do último `)`.
fn pai(stat: &str) -> Option<u32> {
    stat.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

fn resultado(sonda: Child) -> Sondagem {
    match sonda.wait_with_output() {
        Ok(saida) if saida.status.success() => Sondagem::Respondeu(
            String::from_utf8_lossy(&saida.stdout)
                .lines()
                .map(str::to_owned)
                .collect(),
        ),
        Ok(saida) => Sondagem::Falhou(String::from_utf8_lossy(&saida.stderr).trim().to_owned()),
        Err(e) => Sondagem::Falhou(e.to_string()),
    }
}

fn publicar(servico: &Servico) {
    let desejados = servico
        .drivers
        .iter()
        .filter(|(_, d)| d.supervisao.publicado())
        .map(|(chave, d)| (publicacao::nome(chave), publicacao::conteudo(&d.socket)))
        .collect();
    if let Err(e) = publicacao::publicar(&servico.dir_modules, &desejados) {
        eprintln!("{}: {e}", servico.dir_modules.display());
    }
}

fn encerrar(servico: &mut Servico) {
    if let Err(e) = publicacao::publicar(&servico.dir_modules, &BTreeMap::new()) {
        eprintln!("{}: {e}", servico.dir_modules.display());
    }
    for d in servico.drivers.values_mut() {
        matar_sonda(d);
        parar_servidor(d);
    }
}

fn descrever(estado: &Estado) -> String {
    match estado {
        Estado::Iniciando => "iniciando".to_owned(),
        Estado::Publicado(tokens) if tokens.is_empty() => "publicado, sem token".to_owned(),
        Estado::Publicado(tokens) => format!("publicado, tokens: {}", tokens.join(", ")),
        Estado::ComErro(e) => format!("fora da publicação: {e}"),
        Estado::Parado(e) => format!("parado: {e}"),
    }
}

fn iniciar_servidor(d: &mut Driver) {
    for dir in &d.criar_diretorios {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("{}: {}: {e}", d.rotulo, dir.display());
        }
    }
    let _ = fs::remove_file(&d.socket);

    let mut bibliotecas: Vec<PathBuf> = d.modulo.bibliotecas.clone();
    if let Some(atual) = std::env::var_os("LD_LIBRARY_PATH") {
        bibliotecas.extend(std::env::split_paths(&atual));
    }
    let mut cmd = processo::comando_servidor(&d.socket, &d.modulo.modulo);
    cmd.envs(&d.modulo.ambiente);
    if let Ok(caminho) = std::env::join_paths(&bibliotecas)
        && !bibliotecas.is_empty()
    {
        cmd.env("LD_LIBRARY_PATH", caminho);
    }
    match cmd.spawn() {
        Ok(servidor) => d.servidor = Some(servidor),
        Err(e) => eprintln!("{}: p11-kit server: {e}", d.rotulo),
    }
    d.proxima_sonda = Instant::now() + ESPERA_INICIAL;
}

fn parar_servidor(d: &mut Driver) {
    // O servidor apaga o socket ao sair, e as conexões abertas (inclusive uma
    // travada num driver) saem junto, por estarem no mesmo grupo.
    if let Some(mut servidor) = d.servidor.take() {
        processo::parar_grupo(&mut servidor);
    }
}

fn iniciar_sonda(socket: &Path) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("sondar")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    processo::amarrar(&mut cmd);
    cmd.spawn().map_err(|e| e.to_string())
}

fn matar_sonda(d: &mut Driver) {
    if let Some((mut sonda, _)) = d.sonda.take() {
        let _ = sonda.kill();
        let _ = sonda.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_o_pai_mesmo_com_parenteses_e_espacos_no_nome() {
        assert_eq!(
            pai("1234 (p11-kit-remote) S 1200 1234 1234 0 -1"),
            Some(1200)
        );
        assert_eq!(pai("99 (nome (com) espaço) R 7 99 99"), Some(7));
        assert_eq!(pai("lixo"), None);
    }

    #[test]
    fn conta_os_filhos_de_um_processo() {
        // Os filhos de um `sh` só deste teste: os do processo de testes mudam
        // com os outros testes rodando em paralelo.
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "sleep 30 & sleep 30 & wait"]);
        processo::amarrar(&mut cmd);
        let mut pai = cmd.spawn().unwrap();
        let fim = Instant::now() + Duration::from_secs(5);
        while conexoes(pai.id()) < 2 && Instant::now() < fim {
            std::thread::sleep(Duration::from_millis(20));
        }
        let vistos = conexoes(pai.id());
        processo::parar_grupo(&mut pai);
        assert_eq!(vistos, 2);
    }
}
