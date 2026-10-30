//! `ancora sondar <socket>`: lista os tokens de um servidor pelo socket dele,
//! do mesmo jeito que um consumidor faria, sem pedir PIN.
//!
//! Roda num processo à parte porque um driver travado trava quem o chama: o
//! serviço mata a sonda que passar do limite, e segue.

use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use std::path::{Path, PathBuf};

/// O cliente do p11-kit do runtime, que fala com um `p11-kit server`.
pub fn cliente() -> PathBuf {
    PathBuf::from(format!(
        "/usr/lib/{}-linux-gnu/pkcs11/p11-kit-client.so",
        std::env::consts::ARCH
    ))
}

/// `sondar` num subprocesso, desistindo depois de `limite`.
pub fn sondar_com_limite(
    socket: &Path,
    limite: std::time::Duration,
) -> Result<Vec<String>, String> {
    use std::process::{Command, Stdio};
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut filho = Command::new(exe)
        .arg("sondar")
        .arg(socket)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let fim = std::time::Instant::now() + limite;
    loop {
        match filho.try_wait().map_err(|e| e.to_string())? {
            Some(_) => break,
            None if std::time::Instant::now() >= fim => {
                let _ = filho.kill();
                let _ = filho.wait();
                return Err(format!("sem resposta em {} s", limite.as_secs()));
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }
    let saida = filho.wait_with_output().map_err(|e| e.to_string())?;
    if saida.status.success() {
        Ok(String::from_utf8_lossy(&saida.stdout)
            .lines()
            .map(str::to_owned)
            .collect())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned())
    }
}

pub fn sondar(socket: &Path) -> Result<Vec<String>, String> {
    // SAFETY: `sondar` roda no começo de um processo próprio e de vida curta,
    // antes de qualquer outra thread existir.
    unsafe {
        std::env::set_var(
            "P11_KIT_SERVER_ADDRESS",
            format!("unix:path={}", socket.display()),
        )
    };
    let cliente = cliente();
    let p11 = Pkcs11::new(&cliente).map_err(|e| format!("{}: {e}", cliente.display()))?;
    p11.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))
        .map_err(|e| format!("C_Initialize: {e}"))?;
    let slots = p11
        .get_slots_with_token()
        .map_err(|e| format!("C_GetSlotList: {e}"))?;
    let mut rotulos = Vec::new();
    for slot in slots {
        let token = p11
            .get_token_info(slot)
            .map_err(|e| format!("C_GetTokenInfo: {e}"))?;
        // Um slot livre (o SoftHSM sempre deixa um) tem token sem
        // inicializar, sem rótulo e sem certificado.
        if token.token_initialized() {
            rotulos.push(token.label().trim_end().to_owned());
        }
    }
    Ok(rotulos)
}
