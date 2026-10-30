//! O pacote do fabricante que o próprio Âncora baixa (`[pacote]`, contrato 2),
//! no lugar do `extra-data` do Flatpak.
//!
//! O `extra-data` tem dois limites que o Âncora contorna sem mudar o que a
//! extensão faz com o pacote:
//!
//! - quem baixa é o Flatpak, com a validação TLS do sistema e sem como
//!   acrescentar uma CA. O servidor da Softplan manda o próprio certificado no
//!   lugar do intermediário, e a instalação falha. Aqui a extensão traz o
//!   intermediário que falta (`cadeia`), e a conexão é validada contra as CAs
//!   do sistema mais ele: a cadeia é completada, não ignorada;
//! - ele usa uma conexão só. O servidor da DigiCert (SafeNet) limita cada
//!   conexão: medido em 2026-10-30, 226 a 842 KB/s com uma, 2,1 MB/s com 8 e
//!   3 MB/s com 16. Aqui o pacote vem em `CONEXOES` faixas ao mesmo tempo.
//!
//! O resto é o que o Flatpak faria: o pacote inteiro conferido pelo tamanho e
//! pelo sha256, e o `bin/apply_extra` da extensão rodando num sub-sandbox sem
//! rede, que só enxerga a pasta do pacote, com ela como diretório atual. O que
//! ele deixa ali é o que os caminhos `extra/` do descritor alcançam.

use crate::extensao::{Encontrada, Pacote};
use crate::processo;
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Onde ficam os pacotes, relativo à home: a pasta de dados do Âncora, que é a
/// do host, montada no mesmo caminho dentro do sandbox. Um navegador Flatpak
/// recebe permissão de ler a pasta de um assinador pelo mesmo caminho.
pub const PASTA: &str = ".local/share/ancora/pacotes";

/// Quantas faixas do pacote baixar ao mesmo tempo. 16 rendeu 40% a mais que 8
/// no servidor da DigiCert, mas 8 já tira o pacote de minutos para segundos sem
/// abrir conexões demais no servidor de quem distribui.
const CONEXOES: u64 = 8;
/// Abaixo disto por faixa, dividir não compensa.
const MINIMO_POR_FAIXA: u64 = 4 << 20;
/// As CAs do sistema no runtime.
const CAS_DO_SISTEMA: &str = "/etc/ssl/certs/ca-certificates.crt";
/// Quanto o `apply_extra` pode levar.
const LIMITE_APPLY_EXTRA: Duration = Duration::from_secs(600);

/// Se o pacote já foi preparado. A pasta só ganha o nome final depois de o
/// `apply_extra` terminar bem.
pub fn pronto(p: &Pacote) -> bool {
    p.pasta.is_dir()
}

/// Baixa, confere e prepara o pacote, se ainda não estiver pronto. Dois
/// processos que preparam o mesmo pacote (o serviço e a janela, ou dois
/// módulos do mesmo driver) se revezam: o segundo encontra o pacote pronto.
pub fn preparar(p: &Pacote) -> Result<(), String> {
    preparar_com(p, aplicar_no_sandbox)
}

fn preparar_com(
    p: &Pacote,
    aplicar: fn(&Pacote, &Path) -> Result<(), String>,
) -> Result<(), String> {
    let base = p.pasta.parent().ok_or("pacote sem pasta")?;
    fs::create_dir_all(base).map_err(|e| format!("{}: {e}", base.display()))?;
    let caminho_trava = base.join(".trava");
    let trava =
        File::create(&caminho_trava).map_err(|e| format!("{}: {e}", caminho_trava.display()))?;
    trava
        .lock()
        .map_err(|e| format!("{}: {e}", caminho_trava.display()))?;
    if pronto(p) {
        return Ok(());
    }
    let obra = p.pasta.with_extension("parcial");
    let _ = fs::remove_dir_all(&obra);
    fs::create_dir_all(&obra).map_err(|e| format!("{}: {e}", obra.display()))?;
    let feito = (|| {
        baixar(p, &obra)?;
        aplicar(p, &obra)?;
        fs::rename(&obra, &p.pasta).map_err(|e| format!("{}: {e}", p.pasta.display()))
    })();
    if feito.is_err() {
        let _ = fs::remove_dir_all(&obra);
    }
    feito?;
    // As versões anteriores deste pacote.
    if let Ok(entradas) = fs::read_dir(base) {
        for e in entradas.flatten() {
            let caminho = e.path();
            if caminho != p.pasta && e.file_name() != ".trava" {
                let _ = fs::remove_dir_all(&caminho);
            }
        }
    }
    Ok(())
}

/// Baixa o pacote para `obra/<arquivo>`, em faixas quando o servidor aceita,
/// e confere tamanho e sha256. Na `obra` só fica o pacote.
fn baixar(p: &Pacote, obra: &Path) -> Result<(), String> {
    let cas = match &p.cadeia {
        Some(cadeia) => {
            let mut pem = fs::read(CAS_DO_SISTEMA).map_err(|e| format!("{CAS_DO_SISTEMA}: {e}"))?;
            pem.push(b'\n');
            pem.extend(fs::read(cadeia).map_err(|e| format!("{}: {e}", cadeia.display()))?);
            let caminho = obra.join(".cas.pem");
            fs::write(&caminho, pem).map_err(|e| format!("{}: {e}", caminho.display()))?;
            Some(caminho)
        }
        None => None,
    };
    let conexoes = if p.tamanho >= 2 * MINIMO_POR_FAIXA && aceita_faixas(p, cas.as_deref())? {
        CONEXOES.min(p.tamanho / MINIMO_POR_FAIXA)
    } else {
        1
    };
    let destino = obra.join(&p.arquivo);
    let feito = baixar_em_faixas(
        &p.url,
        cas.as_deref(),
        &faixas(p.tamanho, conexoes),
        &destino,
    );
    if let Some(cas) = &cas {
        let _ = fs::remove_file(cas);
    }
    feito?;
    conferir(p, &destino)
}

/// O `curl` com o que todo pedido leva: só HTTPS (redirecionamentos também), e
/// desistir de uma conexão parada em vez de esperar para sempre.
fn curl(url: &str, cas: Option<&Path>) -> Command {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "-f", "-L", "--retry", "3", "--connect-timeout", "30"])
        .args(["--speed-limit", "1024", "--speed-time", "120"]);
    if url.starts_with("https://") {
        cmd.args(["--proto", "=https", "--proto-redir", "=https"]);
    }
    if let Some(cas) = cas {
        cmd.arg("--cacert").arg(cas);
    }
    cmd.arg(url);
    cmd
}

/// Pede o primeiro byte e vê se o servidor (depois dos redirecionamentos)
/// responde 206, com o total que o descritor diz.
fn aceita_faixas(p: &Pacote, cas: Option<&Path>) -> Result<bool, String> {
    let mut cmd = curl(&p.url, cas);
    cmd.args(["-r", "0-0", "-D", "-", "-o", "/dev/null"]);
    let saida = cmd.output().map_err(|e| format!("curl: {e}"))?;
    if !saida.status.success() {
        return Err(format!(
            "{}: {}",
            p.url,
            String::from_utf8_lossy(&saida.stderr).trim()
        ));
    }
    let cabecalhos = String::from_utf8_lossy(&saida.stdout);
    Ok(status_final(&cabecalhos) == Some(206) && total_da_faixa(&cabecalhos) == Some(p.tamanho))
}

/// O status da última resposta (a que vem depois dos redirecionamentos).
fn status_final(cabecalhos: &str) -> Option<u16> {
    cabecalhos
        .lines()
        .rfind(|l| l.starts_with("HTTP/"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// O total do `Content-Range: bytes 0-0/<total>` da última resposta.
fn total_da_faixa(cabecalhos: &str) -> Option<u64> {
    cabecalhos
        .lines()
        .rfind(|l| l.to_ascii_lowercase().starts_with("content-range:"))?
        .rsplit('/')
        .next()?
        .trim()
        .parse()
        .ok()
}

/// `tamanho` bytes divididos em `n` faixas contíguas, de início e fim
/// inclusivos, como o `-r` do curl.
fn faixas(tamanho: u64, n: u64) -> Vec<(u64, u64)> {
    let n = n.clamp(1, tamanho.max(1));
    let parte = tamanho.div_ceil(n);
    (0..n)
        .map(|i| (i * parte, ((i + 1) * parte).min(tamanho) - 1))
        .filter(|(inicio, fim)| inicio <= fim)
        .collect()
}

fn baixar_em_faixas(
    url: &str,
    cas: Option<&Path>,
    faixas: &[(u64, u64)],
    destino: &Path,
) -> Result<(), String> {
    let partes: Vec<PathBuf> = (0..faixas.len())
        .map(|i| destino.with_extension(format!("parte{i}")))
        .collect();
    let mut filhos = Vec::new();
    for ((inicio, fim), parte) in faixas.iter().zip(&partes) {
        let mut cmd = curl(url, cas);
        if faixas.len() > 1 {
            cmd.arg("-r").arg(format!("{inicio}-{fim}"));
        }
        cmd.arg("-o").arg(parte);
        cmd.stderr(std::process::Stdio::piped());
        processo::amarrar(&mut cmd);
        filhos.push(cmd.spawn().map_err(|e| format!("curl: {e}"))?);
    }
    let mut erro = None;
    for filho in filhos {
        let saida = filho.wait_with_output().map_err(|e| e.to_string())?;
        if !saida.status.success() && erro.is_none() {
            erro = Some(format!(
                "{url}: {}",
                String::from_utf8_lossy(&saida.stderr).trim()
            ));
        }
    }
    let juntar = || -> std::io::Result<()> {
        let mut saida = File::create(destino)?;
        for parte in &partes {
            std::io::copy(&mut File::open(parte)?, &mut saida)?;
        }
        Ok(())
    };
    let feito = match erro {
        Some(e) => Err(e),
        None => juntar().map_err(|e| format!("{}: {e}", destino.display())),
    };
    for parte in &partes {
        let _ = fs::remove_file(parte);
    }
    feito
}

fn conferir(p: &Pacote, arquivo: &Path) -> Result<(), String> {
    let tamanho = fs::metadata(arquivo)
        .map_err(|e| format!("{}: {e}", arquivo.display()))?
        .len();
    if tamanho != p.tamanho {
        return Err(format!(
            "o pacote veio com {tamanho} bytes, e a extensão espera {}: o fabricante publicou outra versão?",
            p.tamanho
        ));
    }
    let saida = Command::new("sha256sum")
        .arg(arquivo)
        .output()
        .map_err(|e| format!("sha256sum: {e}"))?;
    let sha = String::from_utf8_lossy(&saida.stdout);
    let sha = sha.split_whitespace().next().unwrap_or_default();
    if !saida.status.success() || sha != p.sha256 {
        return Err(format!(
            "o sha256 do pacote é {sha}, e a extensão espera {}",
            p.sha256
        ));
    }
    Ok(())
}

/// O `apply_extra` da extensão num sub-sandbox do Âncora sem rede, que só
/// pode escrever na pasta do pacote.
fn aplicar_no_sandbox(p: &Pacote, obra: &Path) -> Result<(), String> {
    let mut cmd = Command::new("flatpak-spawn");
    cmd.args(["--sandbox", "--no-network"])
        .arg(format!("--sandbox-expose-path={}", obra.display()))
        .arg(format!("--directory={}", obra.display()))
        .arg(&p.apply_extra);
    let saida = processo::executar(cmd, b"", LIMITE_APPLY_EXTRA)?;
    if !saida.status.success() {
        return Err(format!(
            "apply_extra: {}",
            String::from_utf8_lossy(&saida.stderr).trim()
        ));
    }
    Ok(())
}

/// Apaga os pacotes que nenhuma extensão montada usa: de uma removida, ou de
/// uma versão anterior. Só olha a pasta de pacotes, e só apaga o que não está
/// sendo preparado agora.
pub fn podar(home: &Path, montadas: &[Encontrada]) {
    let em_uso: BTreeSet<PathBuf> = montadas
        .iter()
        .filter_map(|e| e.descritor.as_ref().ok()?.pacote.as_ref())
        .map(|p| p.pasta.clone())
        .collect();
    let Ok(extensoes) = fs::read_dir(home.join(PASTA)) else {
        return;
    };
    for ext in extensoes.flatten() {
        let base = ext.path();
        let Ok(trava) = File::open(base.join(".trava")) else {
            continue;
        };
        if trava.try_lock().is_err() {
            continue;
        }
        let Ok(versoes) = fs::read_dir(&base) else {
            continue;
        };
        let mut restam = 0;
        for v in versoes.flatten() {
            if v.file_name() == ".trava" {
                continue;
            }
            if em_uso.contains(&v.path()) {
                restam += 1;
            } else {
                let _ = fs::remove_dir_all(v.path());
            }
        }
        if restam == 0 {
            let _ = fs::remove_dir_all(&base);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn faixas_cobrem_o_arquivo_inteiro_sem_sobrepor() {
        for (tamanho, n) in [(91_474_258, 8), (10, 3), (10, 10), (10, 20), (1, 8)] {
            let f = faixas(tamanho, n);
            assert_eq!(f[0].0, 0);
            assert_eq!(f.last().unwrap().1, tamanho - 1);
            for par in f.windows(2) {
                assert_eq!(par[0].1 + 1, par[1].0, "{tamanho} em {n}: {f:?}");
            }
            assert!(f.len() as u64 <= n);
        }
        assert_eq!(faixas(100, 1), vec![(0, 99)]);
    }

    #[test]
    fn status_e_total_vem_da_ultima_resposta() {
        let cabecalhos = "HTTP/1.1 302 Found\r\nLocation: https://blob\r\nContent-Length: 0\r\n\r\n\
                          HTTP/2 206 \r\ncontent-range: bytes 0-0/42979168\r\n\r\n";
        assert_eq!(status_final(cabecalhos), Some(206));
        assert_eq!(total_da_faixa(cabecalhos), Some(42_979_168));
        assert_eq!(status_final("HTTP/1.1 200 OK\r\n\r\n"), Some(200));
        assert_eq!(total_da_faixa("HTTP/1.1 200 OK\r\n\r\n"), None);
    }

    fn pacote_de(pasta: &Path, conteudo: &[u8], sha256: &str) -> (PathBuf, Pacote) {
        let origem = pasta.join("origem.bin");
        fs::write(&origem, conteudo).unwrap();
        let p = Pacote {
            url: format!("file://{}", origem.display()),
            sha256: sha256.to_owned(),
            tamanho: conteudo.len() as u64,
            arquivo: "pacote.deb".into(),
            cadeia: None,
            apply_extra: PathBuf::from("/nao/usado"),
            pasta: pasta.join("pacotes/Driver.Teste").join(sha256),
        };
        (origem, p)
    }

    fn sha256(conteudo: &[u8], pasta: &Path) -> String {
        let f = pasta.join("para-sha");
        fs::write(&f, conteudo).unwrap();
        let saida = Command::new("sha256sum").arg(&f).output().unwrap();
        String::from_utf8_lossy(&saida.stdout)
            .split_whitespace()
            .next()
            .unwrap()
            .to_owned()
    }

    fn temporaria(nome: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ancora-pacote-{nome}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn faixas_baixadas_em_paralelo_remontam_o_arquivo() {
        let d = temporaria("faixas");
        let conteudo: Vec<u8> = (0..100_003u32).map(|i| (i * 7 % 251) as u8).collect();
        let (origem, _) = pacote_de(&d, &conteudo, "x");
        let destino = d.join("montado");
        let url = format!("file://{}", origem.display());
        baixar_em_faixas(&url, None, &faixas(conteudo.len() as u64, 5), &destino).unwrap();
        assert_eq!(fs::read(&destino).unwrap(), conteudo);
        // Sem sobra das partes.
        assert_eq!(fs::read_dir(&d).unwrap().count(), 2);
        fs::remove_dir_all(&d).unwrap();
    }

    fn aplicar_de_teste(_: &Pacote, obra: &Path) -> Result<(), String> {
        // O que um apply_extra faz: tira do pacote o que interessa e o apaga.
        fs::remove_file(obra.join("pacote.deb")).map_err(|e| e.to_string())?;
        fs::write(obra.join("libdriver.so"), "ok").map_err(|e| e.to_string())
    }

    fn aplicar_que_falha(_: &Pacote, _: &Path) -> Result<(), String> {
        Err("apply_extra: falhou".into())
    }

    #[test]
    fn preparar_confere_aplica_e_so_entao_publica_a_pasta() {
        let d = temporaria("preparar");
        let conteudo = b"pacote do fabricante".repeat(1000);
        let sha = sha256(&conteudo, &d);
        let (_, p) = pacote_de(&d, &conteudo, &sha);
        // Uma versão anterior, que deve sair.
        let antiga = p.pasta.with_file_name("0000");
        fs::create_dir_all(&antiga).unwrap();

        preparar_com(&p, aplicar_de_teste).unwrap();
        assert!(pronto(&p));
        assert_eq!(
            fs::read_to_string(p.pasta.join("libdriver.so")).unwrap(),
            "ok"
        );
        assert!(!antiga.exists());
        assert!(!p.pasta.with_extension("parcial").exists());
        // Pronto não baixa de novo: o aplicar que falha nem é chamado.
        preparar_com(&p, aplicar_que_falha).unwrap();
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn pacote_diferente_do_esperado_nao_chega_a_ser_aplicado() {
        let d = temporaria("diferente");
        let conteudo = b"outra versao".to_vec();
        let (_, mut p) = pacote_de(&d, &conteudo, &"0".repeat(64));
        let erro = preparar_com(&p, aplicar_de_teste).unwrap_err();
        assert!(erro.contains("sha256"), "{erro}");
        assert!(!pronto(&p));
        assert!(!p.pasta.with_extension("parcial").exists());

        p.tamanho += 1;
        let erro = preparar_com(&p, aplicar_de_teste).unwrap_err();
        assert!(erro.contains("bytes"), "{erro}");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn apply_extra_que_falha_nao_deixa_pacote_pela_metade() {
        let d = temporaria("falha");
        let conteudo = b"pacote".to_vec();
        let sha = sha256(&conteudo, &d);
        let (_, p) = pacote_de(&d, &conteudo, &sha);
        let erro = preparar_com(&p, aplicar_que_falha).unwrap_err();
        assert!(erro.contains("apply_extra"), "{erro}");
        assert!(!pronto(&p));
        assert!(!p.pasta.with_extension("parcial").exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn podar_tira_o_que_nenhuma_extensao_montada_usa() {
        use crate::extensao::{Descritor, Familia};
        let home = temporaria("podar");
        let base = home.join(PASTA);
        let atual = base.join("Driver.SafeNet/novo");
        let antiga = base.join("Driver.SafeNet/velho");
        let removida = base.join("Driver.Removido/qualquer");
        for dir in [&atual, &antiga, &removida] {
            fs::create_dir_all(dir).unwrap();
            File::create(dir.parent().unwrap().join(".trava")).unwrap();
        }
        let (_, mut p) = pacote_de(&home, b"x", "novo");
        p.pasta = atual.clone();
        let montada = Encontrada {
            id: "SafeNet".into(),
            familia: Familia::Driver,
            diretorio: "/app/extensoes/driver/SafeNet".into(),
            descritor: Ok(Descritor {
                nome: "SafeNet".into(),
                fabricante: String::new(),
                descricao: String::new(),
                icone: None,
                criar_diretorios: vec![],
                pkcs11: vec![],
                assinadores: vec![],
                ferramentas: vec![],
                pacote: Some(p),
            }),
        };
        podar(&home, &[montada]);
        assert!(atual.is_dir());
        assert!(!antiga.exists());
        assert!(!base.join("Driver.Removido").exists());
        fs::remove_dir_all(&home).unwrap();
    }
}
