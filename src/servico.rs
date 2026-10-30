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
use crate::publicacao;
use crate::supervisao::{Acao, Estado, Sondagem, Supervisao};
use gtk::glib;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::os::unix::process::CommandExt;
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
    let dir_modules = publicacao::diretorio(&home);
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
                matar_sonda(d);
                Some(Sondagem::Travou)
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

/// Faz o filho morrer junto com o serviço, mesmo que o serviço seja morto
/// com SIGKILL, e o põe num grupo de processos próprio, para que os
/// `p11-kit-remote` de cada conexão saiam junto com o servidor.
fn amarrar_ao_servico(cmd: &mut Command) {
    // SAFETY: entre o fork e o exec só chamamos funções seguras para isso.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            libc::setpgid(0, 0);
            Ok(())
        });
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
    let mut cmd = Command::new("p11-kit");
    cmd.args(["server", "-f", "-n"])
        .arg(&d.socket)
        .arg("--provider")
        .arg(&d.modulo.modulo)
        .arg("pkcs11:")
        .envs(&d.modulo.ambiente)
        .stdin(Stdio::null())
        // O `p11-kit server` escreve no stdout as variáveis para o cliente.
        .stdout(Stdio::null());
    if let Ok(caminho) = std::env::join_paths(&bibliotecas)
        && !bibliotecas.is_empty()
    {
        cmd.env("LD_LIBRARY_PATH", caminho);
    }
    amarrar_ao_servico(&mut cmd);
    match cmd.spawn() {
        Ok(servidor) => d.servidor = Some(servidor),
        Err(e) => eprintln!("{}: p11-kit server: {e}", d.rotulo),
    }
    d.proxima_sonda = Instant::now() + ESPERA_INICIAL;
}

fn parar_servidor(d: &mut Driver) {
    let Some(mut servidor) = d.servidor.take() else {
        return;
    };
    // SIGTERM ao grupo: o servidor apaga o socket ao sair, e as conexões
    // abertas (inclusive uma travada num driver) saem junto.
    let grupo = servidor.id() as libc::pid_t;
    // SAFETY: o grupo é o do filho que este serviço criou.
    unsafe { libc::killpg(grupo, libc::SIGTERM) };
    let limite = Instant::now() + Duration::from_secs(2);
    while Instant::now() < limite {
        if let Ok(Some(_)) = servidor.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: idem.
    unsafe { libc::killpg(grupo, libc::SIGKILL) };
    let _ = servidor.wait();
}

fn iniciar_sonda(socket: &Path) -> Result<Child, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("sondar")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    amarrar_ao_servico(&mut cmd);
    cmd.spawn().map_err(|e| e.to_string())
}

fn matar_sonda(d: &mut Driver) {
    if let Some((mut sonda, _)) = d.sonda.take() {
        let _ = sonda.kill();
        let _ = sonda.wait();
    }
}
