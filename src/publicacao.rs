//! Publicar no p11-kit do host: um `.module` por driver, apontando para o
//! socket do servidor que o Âncora sobe para ele.
//!
//! O p11-kit do host lê `~/.config/pkcs11/modules` em todo processo que
//! carrega o proxy, e o `p11-kit-server.socket` do usuário serve o mesmo
//! conjunto aos aplicativos Flatpak. Um `.module` apontando para um socket
//! morto é pulado sem custo (medido), então o pior caso de uma queda é o
//! token sumir, não travar quem o carrega.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Todo arquivo que o Âncora escreve no diretório de módulos começa assim, e
/// só esses ele apaga.
pub const PREFIXO: &str = "ancora-";
const SUFIXO: &str = ".module";

/// O módulo do token-sentinela (ver `verificacao`). É de quem verifica, que o
/// publica e o tira: a publicação dos drivers não o lê nem o apaga.
pub const SENTINELA: &str = "ancora-verificacao.module";

const MARCA: &str =
    "# Publicado pelo Âncora (dev.lukakuuhaku.Ancora). Reescrito a cada publicação.";

/// Serviços da sessão que carregam todo `.module` do usuário sem precisar de
/// token. Os três primeiros foram medidos no Fedora 44 (login travado,
/// gsd-smartcard em segfault no Ubuntu 26.04); os outros, por prevenção.
pub const DESLIGADO_EM: [&str; 6] = [
    "gsd-smartcard",
    "gnome-software",
    "gvfsd-http",
    "csd-smartcard",
    "gnome-shell",
    "gnome-keyring-daemon",
];

/// Onde o p11-kit do host procura os módulos do usuário. Dentro do sandbox o
/// `XDG_CONFIG_HOME` é o do aplicativo, e o do host não é exportado: vale o
/// padrão, que é para onde a permissão `xdg-config/pkcs11` aponta.
pub fn diretorio(home_host: &Path) -> PathBuf {
    home_host.join(".config/pkcs11/modules")
}

/// O nome do arquivo de um driver: `ancora-safesign.module`.
pub fn nome(chave: &str) -> String {
    format!("{PREFIXO}{}{SUFIXO}", chave.to_lowercase())
}

/// O módulo do token-sentinela vai na frente de todos: o `p11-kit server`
/// encerra a varredura no primeiro token que recusar `C_OpenSession`, e tudo o
/// que vem depois some para os aplicativos Flatpak (p11-kit 0.26.5, iter.c).
/// Com prioridade alta, nenhum outro módulo o esconde.
pub fn conteudo_sentinela(socket: &Path) -> String {
    format!("{}priority: 1000\n", conteudo(socket))
}

pub fn conteudo(socket: &Path) -> String {
    format!(
        "{MARCA}\nremote: unix:path={}\ndisable-in: {}\n",
        socket.display(),
        DESLIGADO_EM.join(" ")
    )
}

/// O que muda no diretório para que ele fique igual ao desejado.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plano {
    pub escrever: Vec<(String, String)>,
    pub apagar: Vec<String>,
}

/// Compara o que está publicado com o que deveria estar. Republicar é
/// idempotente, e o que não corresponde mais a um driver saudável sai.
pub fn planejar(
    existentes: &BTreeMap<String, String>,
    desejados: &BTreeMap<String, String>,
) -> Plano {
    Plano {
        escrever: desejados
            .iter()
            .filter(|(n, c)| existentes.get(*n) != Some(*c))
            .map(|(n, c)| (n.clone(), c.clone()))
            .collect(),
        apagar: existentes
            .keys()
            .filter(|n| !desejados.contains_key(*n))
            .cloned()
            .collect(),
    }
}

/// Tira o módulo do sentinela se a verificação que o publicou morreu sem
/// tirá-lo: o servidor dele não atende. Uma verificação viva tem o servidor
/// de pé, e o módulo fica. Medido em 2026-10-31: a máquina caiu no meio de uma
/// verificação, o `Drop` do sentinela não rodou, e o módulo (prioridade 1000,
/// o primeiro de todos) apontou para um socket morto até ser tirado à mão.
/// Diz se tirou.
pub fn limpar_sentinela_orfao(dir: &Path) -> bool {
    let modulo = dir.join(SENTINELA);
    let Ok(texto) = fs::read_to_string(&modulo) else {
        return false;
    };
    let Some(socket) = texto
        .lines()
        .find_map(|l| l.strip_prefix("remote: unix:path="))
    else {
        return false;
    };
    if std::os::unix::net::UnixStream::connect(socket.trim()).is_ok() {
        return false;
    }
    fs::remove_file(&modulo).is_ok()
}

fn e_nosso(nome: &str) -> bool {
    nome.starts_with(PREFIXO) && nome.ends_with(SUFIXO) && nome != SENTINELA
}

/// Os `ancora-*.module` do diretório, com o conteúdo. Os de outros donos
/// não entram, e diretório inexistente é diretório vazio.
pub fn ler(dir: &Path) -> io::Result<BTreeMap<String, String>> {
    let entradas = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e),
    };
    let mut saida = BTreeMap::new();
    for entrada in entradas {
        let entrada = entrada?;
        let nome = entrada.file_name().to_string_lossy().into_owned();
        if e_nosso(&nome) {
            saida.insert(nome, fs::read_to_string(entrada.path())?);
        }
    }
    Ok(saida)
}

pub fn aplicar(dir: &Path, plano: &Plano) -> io::Result<()> {
    if !plano.escrever.is_empty() {
        fs::create_dir_all(dir)?;
    }
    for (nome, conteudo) in &plano.escrever {
        // Escreve ao lado e troca: quem ler no meio vê o antigo ou o novo,
        // nunca metade. O temporário começa com ponto e não termina em
        // .module, e o p11-kit não o lê.
        let temporario = dir.join(format!(".{nome}.tmp"));
        fs::write(&temporario, conteudo)?;
        fs::rename(&temporario, dir.join(nome))?;
    }
    for nome in &plano.apagar {
        match fs::remove_file(dir.join(nome)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
    }
    Ok(())
}

/// Deixa o diretório com exatamente estes módulos do Âncora.
pub fn publicar(dir: &Path, desejados: &BTreeMap<String, String>) -> io::Result<Plano> {
    let plano = planejar(&ler(dir)?, desejados);
    aplicar(dir, &plano)?;
    Ok(plano)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapa(pares: &[(&str, &str)]) -> BTreeMap<String, String> {
        pares
            .iter()
            .map(|(n, c)| (n.to_string(), c.to_string()))
            .collect()
    }

    #[test]
    fn o_module_aponta_o_socket_e_fica_fora_dos_servicos_da_sessao() {
        let c = conteudo(Path::new("/run/user/1000/ancora/safesign"));
        assert!(c.starts_with("# "));
        assert!(c.contains("\nremote: unix:path=/run/user/1000/ancora/safesign\n"));
        let desligado = c
            .lines()
            .find_map(|l| l.strip_prefix("disable-in: "))
            .expect("linha disable-in");
        for servico in ["gsd-smartcard", "gnome-software", "gvfsd-http"] {
            assert!(
                desligado.split(' ').any(|s| s == servico),
                "falta {servico}"
            );
        }
        // O p11-kit recusa enable-in e disable-in juntos.
        assert!(!c.contains("enable-in"));
    }

    #[test]
    fn o_sentinela_vai_na_frente_de_todos_os_modulos() {
        let c = conteudo_sentinela(Path::new("/run/user/1000/ancora/verificacao"));
        assert!(c.contains("\nremote: unix:path=/run/user/1000/ancora/verificacao\n"));
        assert!(c.ends_with("\npriority: 1000\n"));
        assert!(c.contains("\ndisable-in: "));
    }

    #[test]
    fn o_nome_tem_o_prefixo_e_e_minusculo() {
        assert_eq!(nome("SafeSign"), "ancora-safesign.module");
    }

    #[test]
    fn planejar_escreve_o_que_mudou_e_apaga_o_que_sobrou() {
        let existentes = mapa(&[
            ("ancora-safesign.module", "velho"),
            ("ancora-serproid.module", "igual"),
            ("ancora-safenet.module", "qualquer"),
        ]);
        let desejados = mapa(&[
            ("ancora-safesign.module", "novo"),
            ("ancora-serproid.module", "igual"),
            ("ancora-remoteid.module", "novo"),
        ]);
        assert_eq!(
            planejar(&existentes, &desejados),
            Plano {
                escrever: vec![
                    ("ancora-remoteid.module".into(), "novo".into()),
                    ("ancora-safesign.module".into(), "novo".into()),
                ],
                apagar: vec!["ancora-safenet.module".into()],
            }
        );
    }

    #[test]
    fn republicar_o_mesmo_nao_muda_nada() {
        let m = mapa(&[("ancora-safesign.module", "x")]);
        assert_eq!(planejar(&m, &m), Plano::default());
    }

    struct Dir(PathBuf);

    impl Dir {
        fn novo(nome: &str) -> Dir {
            let p = std::env::temp_dir().join(format!("ancora-{}-{nome}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            Dir(p)
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn publicar_e_despublicar_so_tocam_nos_arquivos_do_ancora() {
        let dir = Dir::novo("publicar");
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join("opensc.module"), "module: opensc-pkcs11.so\n").unwrap();
        fs::write(
            dir.0.join("ancora-velho.module"),
            "de uma publicação anterior",
        )
        .unwrap();
        fs::write(dir.0.join(SENTINELA), "de uma verificação em andamento").unwrap();

        let socket = Path::new("/run/user/1000/ancora/safesign");
        let desejados = mapa(&[(&nome("SafeSign"), &conteudo(socket))]);
        publicar(&dir.0, &desejados).unwrap();

        let mut arquivos: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        arquivos.sort();
        assert_eq!(
            arquivos,
            ["ancora-safesign.module", SENTINELA, "opensc.module"]
        );
        assert_eq!(ler(&dir.0).unwrap(), desejados);

        publicar(&dir.0, &BTreeMap::new()).unwrap();
        let mut arquivos: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        arquivos.sort();
        assert_eq!(arquivos, [SENTINELA, "opensc.module"]);
    }

    #[test]
    fn o_sentinela_de_uma_verificacao_que_morreu_sai_e_o_de_uma_viva_fica() {
        let dir = Dir::novo("sentinela");
        fs::create_dir_all(&dir.0).unwrap();
        // Sem módulo, nada a fazer.
        assert!(!limpar_sentinela_orfao(&dir.0));

        // O servidor morreu: o socket nem existe (a máquina reiniciou).
        let morto = dir.0.join("morto.sock");
        fs::write(dir.0.join(SENTINELA), conteudo_sentinela(&morto)).unwrap();
        assert!(limpar_sentinela_orfao(&dir.0));
        assert!(!dir.0.join(SENTINELA).exists());

        // O socket ficou no disco, mas ninguém atende (o servidor levou SIGKILL).
        drop(std::os::unix::net::UnixListener::bind(&morto).unwrap());
        fs::write(dir.0.join(SENTINELA), conteudo_sentinela(&morto)).unwrap();
        assert!(limpar_sentinela_orfao(&dir.0));
        assert!(!dir.0.join(SENTINELA).exists());

        // Uma verificação em andamento: o servidor atende, e o módulo fica.
        let vivo = dir.0.join("vivo.sock");
        let _ouvinte = std::os::unix::net::UnixListener::bind(&vivo).unwrap();
        fs::write(dir.0.join(SENTINELA), conteudo_sentinela(&vivo)).unwrap();
        assert!(!limpar_sentinela_orfao(&dir.0));
        assert!(dir.0.join(SENTINELA).exists());

        // Conteúdo que não é o nosso: não arrisca apagar.
        fs::write(dir.0.join(SENTINELA), "module: outro.so\n").unwrap();
        assert!(!limpar_sentinela_orfao(&dir.0));
    }

    #[test]
    fn despublicar_sem_diretorio_nao_e_erro() {
        let dir = Dir::novo("sem-diretorio");
        assert_eq!(
            publicar(&dir.0, &BTreeMap::new()).unwrap(),
            Plano::default()
        );
        assert!(!dir.0.exists());
    }
}
