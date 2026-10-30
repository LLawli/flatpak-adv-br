//! Processos filhos: amarrados ao pai, parados em grupo, e com limite de
//! tempo quando o que eles chamam pode travar (um driver, um sandbox).

use std::io::Write;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Faz o filho morrer junto com quem o criou, mesmo num SIGKILL, e o põe num
/// grupo de processos próprio, para que os netos (os `p11-kit-remote` de um
/// servidor) saiam junto com ele.
pub fn amarrar(cmd: &mut Command) {
    // SAFETY: entre o fork e o exec só chamamos funções seguras para isso.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            libc::setpgid(0, 0);
            Ok(())
        });
    }
}

/// Um `p11-kit server` em primeiro plano, servindo `modulo` em `socket`, já
/// amarrado a quem o cria.
pub fn comando_servidor(socket: &std::path::Path, modulo: &std::path::Path) -> Command {
    let mut cmd = Command::new("p11-kit");
    cmd.args(["server", "-f", "-n"])
        .arg(socket)
        .arg("--provider")
        .arg(modulo)
        .arg("pkcs11:")
        .stdin(Stdio::null())
        // Ele escreve no stdout as variáveis para o cliente.
        .stdout(Stdio::null());
    amarrar(&mut cmd);
    cmd
}

/// Para o grupo do filho: SIGTERM, até 2 s, depois SIGKILL.
pub fn parar_grupo(filho: &mut Child) {
    let grupo = filho.id() as libc::pid_t;
    // SAFETY: o grupo é o de um filho deste processo, criado por `amarrar`.
    unsafe { libc::killpg(grupo, libc::SIGTERM) };
    let limite = Instant::now() + Duration::from_secs(2);
    while Instant::now() < limite {
        if let Ok(Some(_)) = filho.try_wait() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: idem.
    unsafe { libc::killpg(grupo, libc::SIGKILL) };
    let _ = filho.wait();
}

/// Roda até o fim, ou até `limite`, com `entrada` na entrada padrão.
pub fn executar(mut cmd: Command, entrada: &[u8], limite: Duration) -> Result<Output, String> {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    amarrar(&mut cmd);
    let mut filho = cmd.spawn().map_err(|e| e.to_string())?;
    if let Some(mut stdin) = filho.stdin.take() {
        stdin.write_all(entrada).map_err(|e| e.to_string())?;
    }
    let fim = Instant::now() + limite;
    loop {
        match filho.try_wait().map_err(|e| e.to_string())? {
            Some(_) => return filho.wait_with_output().map_err(|e| e.to_string()),
            None if Instant::now() >= fim => {
                parar_grupo(&mut filho);
                return Err(format!("sem resposta em {} s", limite.as_secs()));
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executar_passa_a_entrada_e_devolve_a_saida() {
        let mut cmd = Command::new("sh");
        cmd.args(["-c", "tr a-z A-Z"]);
        let saida = executar(cmd, b"ancora", Duration::from_secs(5)).unwrap();
        assert_eq!(saida.stdout, b"ANCORA");
    }

    #[test]
    fn executar_desiste_no_limite() {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        let inicio = Instant::now();
        let erro = executar(cmd, b"", Duration::from_secs(1)).unwrap_err();
        assert!(erro.contains("sem resposta"), "{erro}");
        assert!(inicio.elapsed() < Duration::from_secs(5));
    }
}
