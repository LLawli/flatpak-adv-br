//! `ancora servir`: o serviço que publica os drivers.
//!
//! Para cada módulo PKCS#11 das extensões de driver, sobe um `p11-kit server`
//! em `$XDG_RUNTIME_DIR/ancora/<chave>`, sonda o driver periodicamente num
//! processo à parte que o mantém carregado (`ancora vigiar`, ver `sonda`) e
//! mantém publicado no host (ver `publicacao`) só o que respondeu da última
//! vez. Ao receber SIGTERM, SIGINT ou SIGHUP, ou o pedido de
//! parada da janela, despublica tudo e para os servidores.
//!
//! O que cada driver está fazendo vai para o estado que a janela lê, e um
//! driver que a pessoa desliga é parado e despublicado na hora (ver
//! `estado`).
//!
//! Um driver com `[pacote]` ainda não preparado roda `ancora preparar` num
//! processo à parte antes de subir o servidor (ver `pacote`).
//!
//! As extensões são lidas uma vez, na partida: o Flatpak monta as extensões
//! quando o sandbox começa, e uma instalada depois só aparece num serviço
//! reiniciado.

use crate::estado::{self, Situacao};
use crate::extensao::{self, Familia};
use crate::supervisao::{Acao, Estado, Sondagem, Supervisao};
use crate::{pacote, processo, publicacao, sonda};
use gtk::glib;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime};

/// De quanto em quanto tempo cada driver é sondado.
const INTERVALO: Duration = Duration::from_secs(10);
/// Quanto uma sonda pode levar antes de o driver contar como travado.
const LIMITE_SONDA: Duration = Duration::from_secs(10);
/// A primeira sonda depois de (re)iniciar um servidor, para dar tempo ao socket.
const ESPERA_INICIAL: Duration = Duration::from_secs(1);
/// Quanto esperar a sonda descarregar o driver ao parar.
const ESPERA_PELA_SONDA: Duration = Duration::from_secs(1);
/// O passo do laço que acompanha servidores e sondas.
const PASSO: Duration = Duration::from_millis(200);
/// Depois de um pacote que não pôde ser preparado (sem rede, servidor fora),
/// quanto esperar para tentar de novo.
const NOVA_TENTATIVA_DO_PACOTE: Duration = Duration::from_secs(300);

struct Driver {
    /// O id da extensão, que é o que a pessoa liga e desliga.
    extensao: String,
    /// Como aparece no log: `SafeSign`, ou `SafeSign/2` no segundo módulo.
    rotulo: String,
    modulo: extensao::Pkcs11,
    criar_diretorios: Vec<PathBuf>,
    /// O pacote do fabricante, e o nome com que `ancora preparar` o conhece
    /// (`Driver.SafeNet`).
    pacote: Option<(extensao::Pacote, String)>,
    preparo: Option<Child>,
    proximo_preparo: Instant,
    socket: PathBuf,
    servidor: Option<Child>,
    vigia: Option<Vigia>,
    proxima_sonda: Instant,
    supervisao: Supervisao,
    desligado: bool,
}

struct Servico {
    dir_modules: PathBuf,
    dir_sockets: PathBuf,
    /// A lista de drivers desligados, e quando ela mudou da última vez que
    /// foi lida.
    desligados: PathBuf,
    visto_em: Option<SystemTime>,
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
    let dir_sockets = estado::diretorio();
    fs::create_dir_all(&dir_sockets).map_err(|e| format!("{}: {e}", dir_sockets.display()))?;

    let caminho_trava = estado::caminho_trava(&dir_sockets);
    let trava =
        File::create(&caminho_trava).map_err(|e| format!("{}: {e}", caminho_trava.display()))?;
    trava
        .try_lock()
        .map_err(|_| "já há um serviço do Âncora rodando".to_owned())?;
    estado::descartar_parada(&dir_sockets);

    // Uma queda anterior pode ter deixado módulos publicados. Apontam para
    // sockets mortos, o que não trava ninguém, mas não devem ficar.
    let dir_modules = publicacao::diretorio(&crate::host::home());
    publicacao::publicar(&dir_modules, &BTreeMap::new())
        .map_err(|e| format!("{}: {e}", dir_modules.display()))?;
    // O sentinela fica de fora da publicação (é de quem verifica), mas um de
    // uma verificação que morreu também não deve ficar.
    if publicacao::limpar_sentinela_orfao(&dir_modules) {
        eprintln!("tirado o sentinela de uma verificação que não terminou");
    }

    let extensoes = extensao::encontrar(Path::new(extensao::RAIZ), &home);
    pacote::podar(&home, &extensoes);
    let mut drivers = BTreeMap::new();
    for e in extensoes {
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
                    extensao: e.id.clone(),
                    rotulo,
                    modulo,
                    criar_diretorios: d.criar_diretorios.clone(),
                    pacote: d
                        .pacote
                        .clone()
                        .map(|p| (p, format!("{}.{}", e.familia.ponto(), e.id))),
                    preparo: None,
                    proximo_preparo: Instant::now(),
                    socket: dir_sockets.join(&chave),
                    servidor: None,
                    vigia: None,
                    proxima_sonda: Instant::now(),
                    supervisao: Supervisao::default(),
                    desligado: false,
                },
            );
        }
    }
    if drivers.is_empty() {
        eprintln!("nenhum driver instalado");
    }
    let mut servico = Servico {
        dir_modules,
        dir_sockets,
        desligados: estado::caminho_desligados(&estado::dados()),
        visto_em: None,
        drivers,
        _trava: trava,
    };
    let desligados = ler_desligados(&mut servico);
    for d in servico.drivers.values_mut() {
        d.desligado = desligados.contains(&d.extensao);
    }
    Ok(servico)
}

fn rodar(mut servico: Servico) -> glib::ExitCode {
    for d in servico.drivers.values_mut() {
        if !d.desligado {
            iniciar_servidor(d);
        }
    }
    gravar_estado(&servico);
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
        let laco = laco.clone();
        glib::timeout_add_local(PASSO, move || {
            let mut servico = servico.borrow_mut();
            if estado::parada_pedida(&servico.dir_sockets) {
                eprintln!("parada pedida");
                encerrar(&mut servico);
                laco.quit();
                return glib::ControlFlow::Break;
            }
            acompanhar(&mut servico);
            glib::ControlFlow::Continue
        });
    }

    laco.run();
    glib::ExitCode::SUCCESS
}

/// A lista de drivers desligados, se mudou desde a última leitura.
fn desligados_novos(servico: &mut Servico) -> Option<BTreeSet<String>> {
    let quando = fs::metadata(&servico.desligados)
        .and_then(|m| m.modified())
        .ok();
    if quando == servico.visto_em {
        return None;
    }
    Some(ler_desligados(servico))
}

fn ler_desligados(servico: &mut Servico) -> BTreeSet<String> {
    servico.visto_em = fs::metadata(&servico.desligados)
        .and_then(|m| m.modified())
        .ok();
    estado::ler_lista(&servico.desligados)
}

fn acompanhar(servico: &mut Servico) {
    let mut mudou = false;
    if let Some(desligados) = desligados_novos(servico) {
        for d in servico.drivers.values_mut() {
            let desligar = desligados.contains(&d.extensao);
            if desligar == d.desligado {
                continue;
            }
            d.desligado = desligar;
            if desligar {
                eprintln!("{}: desligado", d.rotulo);
                parar_servidor(d);
                parar_preparo(d);
            } else {
                eprintln!("{}: ligado", d.rotulo);
                d.supervisao = Supervisao::default();
                iniciar_servidor(d);
            }
            mudou = true;
        }
    }
    for d in servico.drivers.values_mut() {
        if d.desligado || d.supervisao.parado() {
            continue;
        }
        // Enquanto o `ancora preparar` existir, mesmo com a pasta já pronta:
        // ela aparece um instante antes de ele sair.
        if d.preparo.is_some() || sem_pacote(d) {
            mudou |= acompanhar_preparo(d);
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
        parar_vigia(d, false);
        return Some(Sondagem::Caiu);
    }
    let Some(v) = &mut d.vigia else {
        // Parada numa sondagem anterior: outra sobe para o próximo pedido.
        if d.servidor.is_some()
            && let Err(e) = iniciar_vigia(d)
        {
            return Some(Sondagem::Falhou(e));
        }
        return None;
    };
    if let Ok(Some(_)) = v.filho.try_wait() {
        let motivo = v.motivo_da_saida();
        d.vigia = None;
        return Some(Sondagem::Falhou(motivo));
    }
    if let Some(inicio) = v.pedido {
        if let Some(linha) = v.linha() {
            v.pedido = None;
            return Some(match sonda::ler_resposta(&linha) {
                Ok(tokens) => Sondagem::Respondeu(tokens),
                Err(e) => Sondagem::Falhou(e),
            });
        }
        if inicio.elapsed() <= LIMITE_SONDA {
            return None;
        }
        // A sonda não é conexão do servidor: qualquer conexão é de um
        // consumidor, que pode estar no meio de um login.
        let ocupado = d.servidor.as_ref().is_some_and(|s| conexoes(s.id()) > 0);
        parar_vigia(d, false);
        return Some(if ocupado {
            Sondagem::Ocupado
        } else {
            Sondagem::Travou
        });
    }
    if Instant::now() >= d.proxima_sonda {
        d.proxima_sonda = Instant::now() + INTERVALO;
        if let Err(e) = v.pedir() {
            parar_vigia(d, false);
            return Some(Sondagem::Falhou(e));
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

fn publicar(servico: &Servico) {
    let desejados = servico
        .drivers
        .iter()
        .filter(|(_, d)| !d.desligado && d.supervisao.publicavel())
        .map(|(chave, d)| (publicacao::nome(chave), publicacao::conteudo(&d.socket)))
        .collect();
    if let Err(e) = publicacao::publicar(&servico.dir_modules, &desejados) {
        eprintln!("{}: {e}", servico.dir_modules.display());
    }
    gravar_estado(servico);
}

fn gravar_estado(servico: &Servico) {
    let drivers = servico
        .drivers
        .values()
        .map(|d| estado::Driver {
            extensao: d.extensao.clone(),
            rotulo: d.rotulo.clone(),
            socket: d.socket.clone(),
            situacao: situacao(d),
        })
        .collect();
    if let Err(e) = estado::gravar(&servico.dir_sockets, &estado::Estado { drivers }) {
        eprintln!("{}: {e}", servico.dir_sockets.display());
    }
}

fn situacao(d: &Driver) -> Situacao {
    if d.desligado {
        return Situacao::Desligado;
    }
    match &d.supervisao.estado {
        Estado::Iniciando => Situacao::Iniciando,
        Estado::Publicado(tokens) if tokens.is_empty() => Situacao::SemToken,
        Estado::Publicado(tokens) => Situacao::Publicado {
            tokens: tokens.clone(),
        },
        Estado::ComErro(motivo) => Situacao::ComErro {
            motivo: motivo.clone(),
        },
        Estado::Parado(motivo) => Situacao::Parado {
            motivo: motivo.clone(),
        },
    }
}

fn encerrar(servico: &mut Servico) {
    if let Err(e) = publicacao::publicar(&servico.dir_modules, &BTreeMap::new()) {
        eprintln!("{}: {e}", servico.dir_modules.display());
    }
    estado::apagar(&servico.dir_sockets);
    for d in servico.drivers.values_mut() {
        parar_servidor(d);
        parar_preparo(d);
    }
}

fn descrever(estado: &Estado) -> String {
    match estado {
        Estado::Iniciando => "iniciando".to_owned(),
        Estado::Publicado(tokens) if tokens.is_empty() => {
            "sem token, fora da publicação".to_owned()
        }
        Estado::Publicado(tokens) => format!("publicado, tokens: {}", tokens.join(", ")),
        Estado::ComErro(e) => format!("fora da publicação: {e}"),
        Estado::Parado(e) => format!("parado: {e}"),
    }
}

/// Se o driver ainda espera o pacote do fabricante.
fn sem_pacote(d: &Driver) -> bool {
    d.pacote.as_ref().is_some_and(|(p, _)| !pacote::pronto(p))
}

/// Acompanha o `ancora preparar` do driver: sobe o servidor quando ele
/// termina bem, e mostra o motivo quando não. Diz se o estado mudou.
fn acompanhar_preparo(d: &mut Driver) -> bool {
    let Some(filho) = &mut d.preparo else {
        if Instant::now() >= d.proximo_preparo {
            iniciar_preparo(d);
        }
        return false;
    };
    match filho.try_wait() {
        Ok(None) => false,
        Ok(Some(_)) => {
            let saida = d
                .preparo
                .take()
                .expect("preparo em andamento")
                .wait_with_output();
            match saida {
                Ok(s) if s.status.success() && !sem_pacote(d) => {
                    eprintln!("{}: pacote do fabricante pronto", d.rotulo);
                    d.supervisao = Supervisao::default();
                    iniciar_servidor(d);
                }
                Ok(s) => falha_do_preparo(d, ultima_linha(&s.stderr)),
                Err(e) => falha_do_preparo(d, e.to_string()),
            }
            true
        }
        Err(e) => {
            d.preparo = None;
            falha_do_preparo(d, e.to_string());
            true
        }
    }
}

fn ultima_linha(stderr: &[u8]) -> String {
    let texto = String::from_utf8_lossy(stderr);
    let linha = texto.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("");
    // "Driver.SafeNet: motivo": o nome já aparece no rótulo.
    linha.split_once(": ").map_or(linha, |(_, m)| m).to_owned()
}

fn falha_do_preparo(d: &mut Driver, motivo: String) {
    eprintln!("{}: pacote do fabricante: {motivo}", d.rotulo);
    d.supervisao.estado = Estado::ComErro(format!("pacote do fabricante: {motivo}"));
    d.proximo_preparo = Instant::now() + NOVA_TENTATIVA_DO_PACOTE;
}

fn iniciar_preparo(d: &mut Driver) {
    let Some((_, nome)) = &d.pacote else { return };
    let exe = std::env::current_exe().unwrap_or_else(|_| "ancora".into());
    let mut cmd = Command::new(exe);
    cmd.args(["preparar", nome])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    processo::amarrar(&mut cmd);
    match cmd.spawn() {
        Ok(filho) => {
            eprintln!("{}: baixando o pacote do fabricante", d.rotulo);
            d.preparo = Some(filho);
        }
        Err(e) => falha_do_preparo(d, e.to_string()),
    }
}

fn parar_preparo(d: &mut Driver) {
    if let Some(mut filho) = d.preparo.take() {
        processo::parar_grupo(&mut filho);
    }
}

fn iniciar_servidor(d: &mut Driver) {
    if sem_pacote(d) {
        d.proximo_preparo = Instant::now();
        acompanhar_preparo(d);
        return;
    }
    for dir in &d.criar_diretorios {
        if let Err(e) = fs::create_dir_all(dir) {
            eprintln!("{}: {}: {e}", d.rotulo, dir.display());
        }
    }
    let _ = fs::remove_file(&d.socket);

    let mut cmd = processo::comando_servidor(&d.socket, &d.modulo.modulo);
    ambiente_do_driver(&mut cmd, &d.modulo);
    match cmd.spawn() {
        Ok(servidor) => d.servidor = Some(servidor),
        Err(e) => eprintln!("{}: p11-kit server: {e}", d.rotulo),
    }
    if let Err(e) = iniciar_vigia(d) {
        eprintln!("{}: sonda: {e}", d.rotulo);
    }
    d.proxima_sonda = Instant::now() + ESPERA_INICIAL;
}

/// O que o driver exige do processo que o carrega: o servidor e a sonda.
fn ambiente_do_driver(cmd: &mut Command, modulo: &extensao::Pkcs11) {
    let mut bibliotecas: Vec<PathBuf> = modulo.bibliotecas.clone();
    if let Some(atual) = std::env::var_os("LD_LIBRARY_PATH") {
        bibliotecas.extend(std::env::split_paths(&atual));
    }
    cmd.envs(&modulo.ambiente);
    if let Ok(caminho) = std::env::join_paths(&bibliotecas)
        && !bibliotecas.is_empty()
    {
        cmd.env("LD_LIBRARY_PATH", caminho);
    }
}

fn parar_servidor(d: &mut Driver) {
    parar_vigia(d, true);
    // O servidor apaga o socket ao sair, e as conexões abertas (inclusive uma
    // travada num driver) saem junto, por estarem no mesmo grupo.
    if let Some(mut servidor) = d.servidor.take() {
        processo::parar_grupo(&mut servidor);
    }
}

/// A sonda de um driver (`ancora vigiar`): o driver carregado uma vez,
/// respondendo a um pedido por vez (ver `sonda::vigiar`).
struct Vigia {
    filho: Child,
    entrada: ChildStdin,
    saida: ChildStdout,
    erros: ChildStderr,
    lido: Vec<u8>,
    /// O fim do stderr, para dizer por que ela saiu.
    ultimos_erros: Vec<u8>,
    /// Quando o pedido em andamento foi feito.
    pedido: Option<Instant>,
}

/// Quanto do stderr da sonda guardar.
const MAXIMO_DE_ERROS: usize = 4096;

impl Vigia {
    fn pedir(&mut self) -> Result<(), String> {
        self.entrada
            .write_all(b"\n")
            .and_then(|_| self.entrada.flush())
            .map_err(|e| format!("sonda: {e}"))?;
        self.pedido = Some(Instant::now());
        Ok(())
    }

    /// Uma resposta inteira, se já chegou. Não bloqueia.
    fn linha(&mut self) -> Option<String> {
        ler_disponivel(&mut self.saida, &mut self.lido);
        self.drenar_erros();
        let fim = self.lido.iter().position(|&b| b == b'\n')?;
        let linha: Vec<u8> = self.lido.drain(..=fim).collect();
        Some(String::from_utf8_lossy(&linha).trim().to_owned())
    }

    fn drenar_erros(&mut self) {
        ler_disponivel(&mut self.erros, &mut self.ultimos_erros);
        if self.ultimos_erros.len() > MAXIMO_DE_ERROS {
            let sobra = self.ultimos_erros.len() - MAXIMO_DE_ERROS;
            self.ultimos_erros.drain(..sobra);
        }
    }

    fn motivo_da_saida(&mut self) -> String {
        self.drenar_erros();
        let texto = String::from_utf8_lossy(&self.ultimos_erros);
        match texto.lines().rfind(|l| !l.trim().is_empty()) {
            Some(l) => l.trim().to_owned(),
            None => match self.filho.try_wait() {
                Ok(Some(s)) => format!("a sonda saiu: {s}"),
                _ => "a sonda saiu".to_owned(),
            },
        }
    }
}

/// Lê o que houver num pipe sem bloqueio.
fn ler_disponivel(pipe: &mut impl Read, destino: &mut Vec<u8>) {
    let mut buf = [0u8; 4096];
    while let Ok(n) = pipe.read(&mut buf) {
        if n == 0 {
            break;
        }
        destino.extend_from_slice(&buf[..n]);
    }
}

fn sem_bloqueio(fd: &impl AsRawFd) {
    let fd = fd.as_raw_fd();
    // SAFETY: só muda as flags de um descritor que este processo tem.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

fn iniciar_vigia(d: &mut Driver) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("vigiar")
        .arg(&d.modulo.modulo)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    ambiente_do_driver(&mut cmd, &d.modulo);
    processo::amarrar(&mut cmd);
    let mut filho = cmd.spawn().map_err(|e| e.to_string())?;
    let (Some(entrada), Some(saida), Some(erros)) =
        (filho.stdin.take(), filho.stdout.take(), filho.stderr.take())
    else {
        processo::parar_grupo(&mut filho);
        return Err("a sonda subiu sem os pipes".into());
    };
    sem_bloqueio(&saida);
    sem_bloqueio(&erros);
    d.vigia = Some(Vigia {
        filho,
        entrada,
        saida,
        erros,
        lido: Vec::new(),
        ultimos_erros: Vec::new(),
        pedido: None,
    });
    Ok(())
}

/// Para a sonda. Com calma, fecha a entrada e dá a ela um instante para
/// descarregar o driver (`C_Finalize`); uma sonda travada é parada direto.
fn parar_vigia(d: &mut Driver, com_calma: bool) {
    let Some(Vigia {
        mut filho, entrada, ..
    }) = d.vigia.take()
    else {
        return;
    };
    drop(entrada);
    if com_calma {
        let limite = Instant::now() + ESPERA_PELA_SONDA;
        while Instant::now() < limite {
            if let Ok(Some(_)) = filho.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    processo::parar_grupo(&mut filho);
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
