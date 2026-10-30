//! Os bancos NSS dos aplicativos: o que o Firefox, o Chromium e o poppler
//! (Papers, Okular) consultam para saber que módulos PKCS#11 carregar.
//!
//! Num banco moderno (cert9.db + key4.db) a lista de módulos é o
//! `pkcs11.txt`, texto puro ao lado dele, e é ele que se edita: dispensa o
//! `modutil`, que nem todo sistema tem.

use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

/// O nome com que o Âncora registra o módulo, e por onde ele o reconhece
/// para atualizar ou remover.
pub const NOME: &str = "Âncora (p11-kit)";

/// O cliente do p11-kit no runtime de um aplicativo Flatpak. É um caminho de
/// dentro do sandbox dele, não deste.
pub const CLIENTE_NO_SANDBOX: &str = "/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so";

/// Bibliotecas que levam aos tokens pelo p11-kit. Registrar uma delas onde o
/// aplicativo já chega aos tokens pelo módulo de confiança faz cada token
/// aparecer duas vezes, e o PIN ser pedido uma vez por cópia.
const DO_P11_KIT: [&str; 2] = ["p11-kit-client.so", "p11-kit-proxy.so"];

fn blocos(texto: &str) -> Vec<String> {
    texto
        .split("\n\n")
        .map(|b| b.trim_matches('\n').to_owned())
        .filter(|b| !b.trim().is_empty())
        .collect()
}

fn campo<'a>(bloco: &'a str, chave: &str) -> Option<&'a str> {
    bloco
        .lines()
        .find_map(|l| l.strip_prefix(chave)?.strip_prefix('='))
}

fn juntar(blocos: &[String]) -> String {
    let mut s = blocos.join("\n\n");
    s.push_str("\n\n");
    s
}

/// O bloco é um registro do p11-kit (cliente ou proxy) de outro dono. O do
/// Âncora fica: no `~/.pki/nssdb`, lido pelos navegadores nativos e pelo
/// Chromium Flatpak, ele aponta o proxy do host por caminho absoluto, que o
/// sandbox não acha e ignora (medido).
fn e_do_p11_kit(bloco: &str) -> bool {
    campo(bloco, "name") != Some(NOME)
        && campo(bloco, "library")
            .map(|l| DO_P11_KIT.iter().any(|b| l.ends_with(b)))
            .unwrap_or(false)
}

/// O `pkcs11.txt` com o módulo do Âncora; `None` se já estava assim.
pub fn com_modulo(texto: &str, biblioteca: &str) -> Option<String> {
    let novo = format!("library={biblioteca}\nname={NOME}\nNSS=trustOrder=100");
    let mut lista = blocos(texto);
    match lista.iter().position(|b| campo(b, "name") == Some(NOME)) {
        Some(i) if lista[i] == novo => return None,
        Some(i) => lista[i] = novo,
        None => lista.push(novo),
    }
    Some(juntar(&lista))
}

/// O `pkcs11.txt` sem o módulo do Âncora; `None` se ele não estava lá.
pub fn sem_modulo(texto: &str) -> Option<String> {
    sem(texto, |b| campo(b, "name") == Some(NOME))
}

/// O `pkcs11.txt` sem nenhum registro do p11-kit; `None` se não havia.
pub fn sem_p11_kit(texto: &str) -> Option<String> {
    sem(texto, e_do_p11_kit)
}

fn sem(texto: &str, tirar: impl Fn(&str) -> bool) -> Option<String> {
    let lista = blocos(texto);
    let restantes: Vec<String> = lista.iter().filter(|b| !tirar(b)).cloned().collect();
    (restantes.len() != lista.len()).then(|| juntar(&restantes))
}

pub fn tem_modulo(texto: &str) -> bool {
    blocos(texto).iter().any(|b| campo(b, "name") == Some(NOME))
}

pub fn tem_p11_kit(texto: &str) -> bool {
    blocos(texto).iter().any(|b| e_do_p11_kit(b))
}

/// Os bancos (diretórios com `cert9.db`) em `raiz` e um nível abaixo.
/// Procurar o arquivo, em vez de ler o `profiles.ini`, sobrevive ao Firefox
/// mudar o perfil de lugar.
pub fn bancos(raiz: &Path) -> Vec<PathBuf> {
    let mut saida = Vec::new();
    if raiz.join("cert9.db").is_file() {
        saida.push(raiz.to_path_buf());
    }
    if let Ok(entradas) = fs::read_dir(raiz) {
        saida.extend(
            entradas
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.join("cert9.db").is_file()),
        );
    }
    saida.sort();
    saida
}

/// Aplica `f` ao `pkcs11.txt` do banco e grava se ele mudou. Devolve se
/// mudou.
pub fn editar(banco: &Path, f: impl Fn(&str) -> Option<String>) -> io::Result<bool> {
    let arquivo = banco.join("pkcs11.txt");
    let texto = match fs::read_to_string(&arquivo) {
        Ok(t) => t,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let Some(novo) = f(&texto) else {
        return Ok(false);
    };
    let temporario = banco.join(".pkcs11.txt.ancora.tmp");
    fs::write(&temporario, novo)?;
    fs::rename(&temporario, &arquivo)?;
    Ok(true)
}

pub fn ler(banco: &Path) -> String {
    fs::read_to_string(banco.join("pkcs11.txt")).unwrap_or_default()
}

/// O perfil está aberto por um Firefox: ele segura um lock POSIX no
/// `.parentlock`. (O link `lock` pode ter sobrado de um Firefox que fechou
/// mal, e não conta.) Editar o banco com ele aberto não vale: o Firefox
/// sobrescreve o arquivo ao sair.
pub fn em_uso(perfil: &Path) -> bool {
    let Ok(arquivo) = fs::File::open(perfil.join(".parentlock")) else {
        return false;
    };
    // SAFETY: `flock` é uma estrutura C simples, zerada e preenchida aqui.
    let mut trava: libc::flock = unsafe { std::mem::zeroed() };
    trava.l_type = libc::F_WRLCK as libc::c_short;
    trava.l_whence = libc::SEEK_SET as libc::c_short;
    // SAFETY: o descritor é válido enquanto `arquivo` vive.
    let r = unsafe { libc::fcntl(arquivo.as_raw_fd(), libc::F_GETLK, &mut trava) };
    r == 0 && trava.l_type != libc::F_UNLCK as libc::c_short
}

/// Cria um banco NSS vazio em `dir`, com a libnss3 do runtime.
pub fn criar_banco(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let config =
        std::ffi::CString::new(format!("sql:{}", dir.display())).map_err(|e| e.to_string())?;
    type Inicializar = unsafe extern "C" fn(
        *const libc::c_char,
        *const libc::c_char,
        *const libc::c_char,
        *const libc::c_char,
        u32,
    ) -> libc::c_int;
    type Encerrar = unsafe extern "C" fn() -> libc::c_int;

    // O NSS é global no processo: dois NSS_Initialize/NSS_Shutdown ao mesmo
    // tempo derrubam o processo (medido: 8 quedas em 20 rodadas dos testes em
    // paralelo, nenhuma com esta trava).
    static NSS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _vez = NSS.lock().unwrap_or_else(|e| e.into_inner());

    // SAFETY: as assinaturas são as de nss.h, e a trava acima garante que só
    // uma chamada usa o NSS por vez.
    unsafe {
        // Nunca descarregada: o NSS deixa ganchos que apontam para ela.
        let nss: &'static libloading::Library = Box::leak(Box::new(
            libloading::Library::new("libnss3.so").map_err(|e| e.to_string())?,
        ));
        let inicializar: libloading::Symbol<Inicializar> =
            nss.get(b"NSS_Initialize\0").map_err(|e| e.to_string())?;
        let encerrar: libloading::Symbol<Encerrar> =
            nss.get(b"NSS_Shutdown\0").map_err(|e| e.to_string())?;
        if inicializar(
            config.as_ptr(),
            c"".as_ptr(),
            c"".as_ptr(),
            c"secmod.db".as_ptr(),
            0,
        ) != 0
        {
            return Err(format!("NSS_Initialize falhou em {}", dir.display()));
        }
        encerrar();
    }
    if dir.join("cert9.db").is_file() {
        Ok(())
    } else {
        Err(format!("{}: o NSS não criou o cert9.db", dir.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERNO: &str = "library=\nname=NSS Internal PKCS #11 Module\nparameters=configdir='sql:/x' certPrefix='' keyPrefix='' secmod='secmod.db'\nNSS=Flags=internal,critical trustOrder=75 cipherOrder=100\n\n";

    #[test]
    fn registra_uma_vez_e_preserva_o_modulo_interno() {
        let um = com_modulo(INTERNO, CLIENTE_NO_SANDBOX).unwrap();
        assert!(um.starts_with("library=\nname=NSS Internal"));
        assert!(um.contains(&format!("library={CLIENTE_NO_SANDBOX}\nname={NOME}\n")));
        assert!(tem_modulo(&um));
        assert_eq!(com_modulo(&um, CLIENTE_NO_SANDBOX), None);
        assert_eq!(um.matches(NOME).count(), 1);
    }

    #[test]
    fn registrar_com_outra_biblioteca_atualiza_no_lugar() {
        let um = com_modulo(INTERNO, "/velho.so").unwrap();
        let dois = com_modulo(&um, CLIENTE_NO_SANDBOX).unwrap();
        assert!(!dois.contains("/velho.so"));
        assert_eq!(dois.matches(NOME).count(), 1);
    }

    #[test]
    fn remover_tira_so_o_do_ancora() {
        let outro = "library=/usr/lib/opensc-pkcs11.so\nname=OpenSC\n";
        let texto = com_modulo(&format!("{INTERNO}{outro}"), CLIENTE_NO_SANDBOX).unwrap();
        let sem = sem_modulo(&texto).unwrap();
        assert!(!tem_modulo(&sem));
        assert!(sem.contains("name=OpenSC") && sem.contains("NSS Internal"));
        assert_eq!(sem_modulo(&sem), None);
    }

    #[test]
    fn acha_e_tira_registros_do_p11_kit_de_outros_donos() {
        let do_ancora = com_modulo("", "/usr/lib/p11-kit-proxy.so").unwrap();
        let texto = format!(
            "{INTERNO}library=/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so\nname=Tessera (p11-kit)\n\nlibrary=p11-kit-proxy.so\nname=advbr-proxy\n\nlibrary=/usr/lib/opensc-pkcs11.so\nname=OpenSC\n\n{do_ancora}"
        );
        assert!(tem_p11_kit(&texto));
        let limpo = sem_p11_kit(&texto).unwrap();
        assert!(!tem_p11_kit(&limpo));
        assert!(limpo.contains("OpenSC") && limpo.contains("NSS Internal"));
        assert!(tem_modulo(&limpo), "o registro do Âncora não pode sair");
        assert!(!tem_p11_kit(INTERNO));
        assert!(!tem_p11_kit(&do_ancora));
    }

    struct Dir(PathBuf);
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn dir(nome: &str) -> Dir {
        let p = std::env::temp_dir().join(format!("ancora-nss-{}-{nome}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Dir(p)
    }

    #[test]
    fn acha_bancos_na_raiz_e_um_nivel_abaixo() {
        let d = dir("bancos");
        for p in ["a.default", "b.default-release", "sem-banco"] {
            fs::create_dir_all(d.0.join(p)).unwrap();
        }
        fs::write(d.0.join("a.default/cert9.db"), "").unwrap();
        fs::write(d.0.join("b.default-release/cert9.db"), "").unwrap();
        assert_eq!(
            bancos(&d.0),
            vec![d.0.join("a.default"), d.0.join("b.default-release")]
        );
        assert!(bancos(&d.0.join("nao-existe")).is_empty());
    }

    #[test]
    fn editar_grava_so_quando_muda() {
        let d = dir("editar");
        fs::write(d.0.join("pkcs11.txt"), INTERNO).unwrap();
        assert!(editar(&d.0, |t| com_modulo(t, CLIENTE_NO_SANDBOX)).unwrap());
        assert!(!editar(&d.0, |t| com_modulo(t, CLIENTE_NO_SANDBOX)).unwrap());
        assert!(tem_modulo(&ler(&d.0)));
    }

    #[test]
    fn perfil_sem_firefox_aberto_nao_esta_em_uso() {
        let d = dir("lock");
        assert!(!em_uso(&d.0));
        fs::write(d.0.join(".parentlock"), "").unwrap();
        assert!(!em_uso(&d.0));
    }

    #[test]
    fn perfil_com_lock_de_outro_processo_esta_em_uso() {
        let d = dir("lock-outro");
        let lock = d.0.join(".parentlock");
        fs::write(&lock, "").unwrap();
        // Um lock POSIX não conflita com o próprio processo: quem segura tem
        // de ser outro, como o Firefox é.
        let mut filho = std::process::Command::new("python3")
            .args([
                "-I",
                "-c",
                "import fcntl,sys,time; f=open(sys.argv[1],'w'); fcntl.lockf(f, fcntl.LOCK_EX); print('ok', flush=True); time.sleep(30)",
            ])
            .arg(&lock)
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("python3");
        let mut linha = String::new();
        std::io::BufRead::read_line(
            &mut std::io::BufReader::new(filho.stdout.as_mut().unwrap()),
            &mut linha,
        )
        .unwrap();
        let resultado = em_uso(&d.0);
        let _ = filho.kill();
        let _ = filho.wait();
        assert!(resultado, "o lock do outro processo não foi visto");
    }

    #[test]
    fn cria_um_banco_vazio_de_verdade() {
        let d = dir("criar");
        let banco = d.0.join("ancora.default");
        criar_banco(&banco).unwrap();
        assert!(banco.join("cert9.db").is_file());
        assert!(banco.join("key4.db").is_file());
        assert!(!tem_p11_kit(&ler(&banco)));
    }
}
