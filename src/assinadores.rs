//! Os assinadores nos navegadores: o Lacuna Web PKI, o Softplan WebSigner e
//! afins, que a extensão do navegador abre por native messaging.
//!
//! O assinador roda no ambiente de quem o chama:
//!
//! - **navegador Flatpak**: dentro do sandbox do próprio navegador, que ganha
//!   permissão de ler a pasta da extensão do Âncora (e o X11, que os
//!   assinadores Avalonia exigem). Nenhuma permissão de sair do sandbox, ao
//!   contrário do flatpak-adv-br, que dava ao navegador o
//!   `org.freedesktop.Flatpak`. É o modelo do Sigillum, testado com token real
//!   em 29/10/2026.
//! - **navegador nativo**: dentro do sandbox do Âncora (`ancora assinar`), sem
//!   depender das bibliotecas do sistema.
//!
//! Nos dois, a extensão manda o assinador carregar uma biblioteca PKCS#11 pelo
//! nome (`libaetpkss.so.3`), e esse nome é um link para o cliente do p11-kit,
//! que chega ao servidor do sistema e aos tokens de todos os drivers.

use crate::extensao::{self, Assinador, Familia};
use crate::{APP_ID, host, nss, permissao};
use permissao::Entrada;
use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tipo {
    Firefox,
    Chromium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Navegador {
    pub nome: &'static str,
    pub tipo: Tipo,
    /// O id Flatpak; `None` para o nativo.
    pub flatpak: Option<&'static str>,
    /// Onde ele procura os manifestos, relativo à home (nativo) ou a
    /// `~/.var/app/<id>` (Flatpak).
    pub manifestos: &'static [&'static str],
    /// O que precisa existir para o nativo contar como instalado.
    pub marcas: &'static [&'static str],
}

const FIREFOX: &[&str] = &[
    ".mozilla/native-messaging-hosts",
    ".config/mozilla/native-messaging-hosts",
];
const FIREFOX_FLATPAK: &[&str] = &[
    ".mozilla/native-messaging-hosts",
    "config/mozilla/native-messaging-hosts",
];

/// Os navegadores e onde cada um lê os manifestos (levantado no
/// flatpak-adv-br e no Sigillum). O Firefox 147 passou o perfil para
/// `.config/mozilla`; o manifesto vai nos dois lugares.
pub const NAVEGADORES: &[Navegador] = &[
    Navegador {
        nome: "Firefox",
        tipo: Tipo::Firefox,
        flatpak: None,
        manifestos: FIREFOX,
        marcas: &[".mozilla/firefox", ".config/mozilla/firefox"],
    },
    Navegador {
        nome: "Google Chrome",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/google-chrome/NativeMessagingHosts"],
        marcas: &[".config/google-chrome"],
    },
    Navegador {
        nome: "Chromium",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/chromium/NativeMessagingHosts"],
        marcas: &[".config/chromium"],
    },
    Navegador {
        nome: "Brave",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/BraveSoftware/Brave-Browser/NativeMessagingHosts"],
        marcas: &[".config/BraveSoftware/Brave-Browser"],
    },
    Navegador {
        nome: "Microsoft Edge",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/microsoft-edge/NativeMessagingHosts"],
        marcas: &[".config/microsoft-edge"],
    },
    Navegador {
        nome: "Vivaldi",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/vivaldi/NativeMessagingHosts"],
        marcas: &[".config/vivaldi"],
    },
    Navegador {
        nome: "Opera",
        tipo: Tipo::Chromium,
        flatpak: None,
        manifestos: &[".config/opera/NativeMessagingHosts"],
        marcas: &[".config/opera"],
    },
    Navegador {
        nome: "Firefox",
        tipo: Tipo::Firefox,
        flatpak: Some("org.mozilla.firefox"),
        manifestos: FIREFOX_FLATPAK,
        marcas: &[],
    },
    Navegador {
        nome: "Chromium",
        tipo: Tipo::Chromium,
        flatpak: Some("org.chromium.Chromium"),
        manifestos: &["config/chromium/NativeMessagingHosts"],
        marcas: &[],
    },
    Navegador {
        nome: "Google Chrome",
        tipo: Tipo::Chromium,
        flatpak: Some("com.google.Chrome"),
        manifestos: &["config/google-chrome/NativeMessagingHosts"],
        marcas: &[],
    },
    Navegador {
        nome: "Microsoft Edge",
        tipo: Tipo::Chromium,
        flatpak: Some("com.microsoft.Edge"),
        manifestos: &["config/microsoft-edge/NativeMessagingHosts"],
        marcas: &[],
    },
    Navegador {
        nome: "Brave",
        tipo: Tipo::Chromium,
        flatpak: Some("com.brave.Browser"),
        manifestos: &["config/BraveSoftware/Brave-Browser/NativeMessagingHosts"],
        marcas: &[],
    },
];

/// Os navegadores presentes: o nativo, pela pasta de perfil; o Flatpak, por
/// estar instalado e já ter sido aberto (a pasta dele existe).
pub fn presentes(home: &Path, flatpaks: &BTreeSet<String>) -> Vec<Navegador> {
    NAVEGADORES
        .iter()
        .copied()
        .filter(|n| match n.flatpak {
            None => n.marcas.iter().any(|m| home.join(m).is_dir()),
            Some(id) => flatpaks.contains(id) && host::var_app(home, id).is_dir(),
        })
        .collect()
}

/// Um assinador instalado, com o lugar da extensão no host.
#[derive(Debug, Clone)]
pub struct Instalado {
    pub id: String,
    pub assinador: Assinador,
    /// O diretório da extensão montado no sandbox do Âncora
    /// (`/app/extensoes/assinador/<id>`).
    pub dentro: PathBuf,
    /// A raiz da instalação da extensão no host (o que o navegador Flatpak
    /// recebe permissão de ler) e os arquivos dela, por um caminho que não
    /// muda quando a extensão é atualizada.
    pub raiz_no_host: PathBuf,
    pub arquivos_no_host: PathBuf,
}

impl Instalado {
    /// O caminho no host de um arquivo da extensão.
    fn no_host(&self, dentro: &Path) -> PathBuf {
        match dentro.strip_prefix(&self.dentro) {
            Ok(rel) => self.arquivos_no_host.join(rel),
            Err(_) => dentro.to_path_buf(),
        }
    }
}

/// Os assinadores das extensões instaladas, com onde cada extensão está no
/// host (perguntado ao host).
pub fn instalados() -> Vec<Result<Instalado, String>> {
    let mut saida = Vec::new();
    for e in extensao::montadas() {
        if e.familia != Familia::Assinador {
            continue;
        }
        let referencia = format!("{APP_ID}.{}.{}", e.familia.ponto(), e.id);
        let d = match &e.descritor {
            Ok(d) => d,
            Err(erro) => {
                saida.push(Err(format!("{referencia}: {erro}")));
                continue;
            }
        };
        let lugar = match local_no_host(&referencia) {
            Ok(l) => l,
            Err(erro) => {
                saida.push(Err(format!("{referencia}: {erro}")));
                continue;
            }
        };
        for a in &d.assinadores {
            saida.push(Ok(Instalado {
                id: e.id.clone(),
                assinador: a.clone(),
                dentro: e.diretorio.clone(),
                raiz_no_host: lugar.0.clone(),
                arquivos_no_host: lugar.1.clone(),
            }));
        }
    }
    saida
}

/// De `flatpak info --show-location` (`.../runtime/<ref>/<arq>/<ramo>/<commit>`)
/// para a raiz da instalação da extensão e os arquivos pelo link `active`.
fn local_no_host(referencia: &str) -> Result<(PathBuf, PathBuf), String> {
    let saida = Command::new("flatpak-spawn")
        .args(["--host", "flatpak", "info", "--show-location", referencia])
        .output()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    if !saida.status.success() {
        return Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned());
    }
    lugar(String::from_utf8_lossy(&saida.stdout).trim())
}

fn lugar(local: &str) -> Result<(PathBuf, PathBuf), String> {
    let commit = Path::new(local);
    let ramo = commit.parent().ok_or("local sem ramo")?;
    let raiz = ramo
        .parent()
        .and_then(Path::parent)
        .ok_or("local sem raiz")?;
    Ok((raiz.to_path_buf(), ramo.join("active/files")))
}

/// O manifesto do fabricante com o `path` trocado. Recusa o formato da outra
/// família: um navegador que recebe o formato errado o ignora sem avisar.
pub fn manifesto(fabricante: &str, tipo: Tipo, lancador: &Path) -> Result<String, String> {
    let mut json: serde_json::Value =
        serde_json::from_str(fabricante).map_err(|e| format!("manifesto ilegível: {e}"))?;
    let exigido = match tipo {
        Tipo::Firefox => "allowed_extensions",
        Tipo::Chromium => "allowed_origins",
    };
    let objeto = json.as_object_mut().ok_or("o manifesto não é um objeto")?;
    if !objeto.get(exigido).is_some_and(serde_json::Value::is_array) {
        return Err(format!("o manifesto não tem {exigido}"));
    }
    objeto.insert("path".into(), lancador.display().to_string().into());
    Ok(serde_json::to_string_pretty(&json).expect("JSON serializa") + "\n")
}

/// O lançador num navegador Flatpak: roda o assinador ali dentro.
pub fn lancador_flatpak(
    executavel: &Path,
    bibliotecas: &Path,
    casa: &Path,
    ambiente: &[(String, String)],
) -> String {
    let mut s = String::from(
        "#!/bin/sh\n# Gerado pelo Âncora. O assinador roda no sandbox deste navegador.\n",
    );
    s += &format!(
        "export LD_LIBRARY_PATH='{}'\"${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}}\"\n",
        bibliotecas.display()
    );
    s += &format!("export HOME='{}'\nmkdir -p \"$HOME\"\n", casa.display());
    for (k, v) in ambiente {
        s += &format!("export {k}='{}'\n", v.replace('\'', "'\\''"));
    }
    s += &format!("exec '{}' \"$@\"\n", executavel.display());
    s
}

/// O lançador num navegador nativo: roda o assinador no sandbox do Âncora.
pub fn lancador_nativo(host_nm: &str) -> String {
    format!(
        "#!/bin/sh\n# Gerado pelo Âncora. O assinador roda no sandbox do Âncora.\nexec flatpak run --command=ancora {APP_ID} assinar '{host_nm}' \"$@\"\n"
    )
}

/// O que a publicação escreve: arquivos (com o conteúdo, ou o destino de um
/// link) e permissões de aplicativos Flatpak.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Plano {
    pub arquivos: Vec<(PathBuf, Conteudo)>,
    pub permissoes: Vec<(String, Entrada)>,
    /// Navegador Flatpak sem a permissão do socket do sistema: o assinador
    /// abre, mas não acha tokens até `ancora configurar <id>`.
    pub sem_tokens: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conteudo {
    Texto(String),
    Executavel(String),
    Link(PathBuf),
    /// Uma pasta do Âncora (a casa de um assinador): criada ao publicar e
    /// apagada inteira ao despublicar.
    Pasta,
}

/// As pastas que podem sair quando ficam vazias: as do Âncora e as de
/// manifestos. A pasta do navegador nunca.
const PASTAS_PODAVEIS: [&str; 6] = [
    "lib",
    "casa",
    "ancora",
    "assinadores",
    "native-messaging-hosts",
    "NativeMessagingHosts",
];

fn podar(caminho: &Path) {
    let mut pai = caminho.parent();
    while let Some(p) = pai {
        let podavel = p
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| PASTAS_PODAVEIS.contains(&n));
        if !podavel || fs::remove_dir(p).is_err() {
            break;
        }
        pai = p.parent();
    }
}

/// Monta a publicação de todos os assinadores em todos os navegadores.
pub fn planejar(
    home: &Path,
    navegadores: &[Navegador],
    assinadores: &[Instalado],
) -> Result<Plano, String> {
    let mut plano = Plano::default();
    for n in navegadores {
        for i in assinadores {
            let a = &i.assinador;
            let fabricante = match n.tipo {
                Tipo::Firefox => &a.manifesto_firefox,
                Tipo::Chromium => &a.manifesto_chromium,
            };
            let Some(fabricante) = fabricante else {
                continue;
            };
            let fabricante = fs::read_to_string(fabricante)
                .map_err(|e| format!("{}: {e}", fabricante.display()))?;
            let (base, lancador) = match n.flatpak {
                Some(id) => {
                    let base = host::var_app(home, id);
                    let pasta = base.join("data/ancora");
                    let lancador = pasta.join(&a.host);
                    let bibliotecas = pasta.join("lib");
                    for nome in &a.pkcs11 {
                        plano.arquivos.push((
                            bibliotecas.join(nome),
                            Conteudo::Link(nss::CLIENTE_NO_SANDBOX.into()),
                        ));
                    }
                    let ambiente: Vec<_> = a
                        .ambiente
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    let casa = pasta.join("casa").join(&a.host);
                    plano.arquivos.push((casa.clone(), Conteudo::Pasta));
                    let texto =
                        lancador_flatpak(&i.no_host(&a.executavel), &bibliotecas, &casa, &ambiente);
                    plano
                        .arquivos
                        .push((lancador.clone(), Conteudo::Executavel(texto)));
                    plano.permissoes.push((
                        id.to_owned(),
                        Entrada::arquivos(format!("{}:ro", i.raiz_no_host.display())),
                    ));
                    if a.x11 {
                        plano
                            .permissoes
                            .push((id.to_owned(), Entrada::socket("x11")));
                    }
                    if !permissao::tem_em(home, id, &Entrada::socket_do_sistema()) {
                        plano.sem_tokens.insert(id.to_owned());
                    }
                    (base, lancador)
                }
                None => {
                    let lancador = home.join(".local/share/ancora/assinadores").join(&a.host);
                    plano.arquivos.push((
                        lancador.clone(),
                        Conteudo::Executavel(lancador_nativo(&a.host)),
                    ));
                    (home.to_path_buf(), lancador)
                }
            };
            let json = manifesto(&fabricante, n.tipo, &lancador)
                .map_err(|e| format!("{} ({}): {e}", a.host, n.nome))?;
            for dir in n.manifestos {
                plano.arquivos.push((
                    base.join(dir).join(format!("{}.json", a.host)),
                    Conteudo::Texto(json.clone()),
                ));
            }
        }
    }
    plano.arquivos.sort_by(|a, b| a.0.cmp(&b.0));
    plano.arquivos.dedup_by(|a, b| a.0 == b.0);
    plano.permissoes.sort();
    plano.permissoes.dedup();
    Ok(plano)
}

fn arquivo_ligados(dados: &Path) -> PathBuf {
    dados.join("assinadores-ligados")
}

/// As extensões de assinador que a pessoa quer nos navegadores.
pub fn ligados(dados: &Path) -> BTreeSet<String> {
    crate::estado::ler_lista(&arquivo_ligados(dados))
}

pub fn ligar(dados: &Path, ids: &BTreeSet<String>) -> Result<(), String> {
    crate::estado::gravar_lista(&arquivo_ligados(dados), ids).map_err(|e| e.to_string())
}

/// Publica os assinadores ligados em todo navegador presente, e tira o resto.
/// É o que se roda quando a escolha muda, e de novo quando um navegador
/// aparece.
pub fn sincronizar(home: &Path, dados: &Path) -> Result<Plano, String> {
    let ligados = ligados(dados);
    if ligados.is_empty() {
        let plano = Plano::default();
        aplicar(home, dados, &plano)?;
        return Ok(plano);
    }
    let flatpaks = crate::aplicativos::instalados()?;
    let navegadores = presentes(home, &flatpaks);
    let mut escolhidos = Vec::new();
    for i in instalados() {
        let i = i?;
        if ligados.contains(&i.id) {
            escolhidos.push(i);
        }
    }
    let plano = planejar(home, &navegadores, &escolhidos)?;
    aplicar(home, dados, &plano)?;
    Ok(plano)
}

fn arquivo_registro(dados: &Path) -> PathBuf {
    dados.join("assinadores")
}

/// O que a publicação anterior escreveu: um caminho por linha.
fn escritos(dados: &Path) -> BTreeSet<PathBuf> {
    fs::read_to_string(arquivo_registro(dados))
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// As permissões que a publicação anterior pediu, guardadas junto.
fn arquivo_permissoes(dados: &Path) -> PathBuf {
    dados.join("assinadores-permissoes")
}

fn permissoes_anteriores(dados: &Path) -> Vec<(String, Entrada)> {
    fs::read_to_string(arquivo_permissoes(dados))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut c = l.splitn(3, '\t');
            let app = c.next()?.to_owned();
            let chave = c.next()?;
            let valor = c.next()?.to_owned();
            Some((
                app,
                match chave {
                    "sockets" => Entrada::socket(valor),
                    _ => Entrada::arquivos(valor),
                },
            ))
        })
        .collect()
}

/// Deixa o disco igual ao plano: escreve o que falta, apaga o que a
/// publicação anterior escreveu e não está mais no plano, e o mesmo com as
/// permissões. Publicar de novo é idempotente; despublicar é aplicar um plano
/// vazio.
pub fn aplicar(home: &Path, dados: &Path, plano: &Plano) -> Result<(), String> {
    let novos: BTreeSet<PathBuf> = plano.arquivos.iter().map(|(p, _)| p.clone()).collect();
    for velho in escritos(dados).difference(&novos) {
        let r = match fs::symlink_metadata(velho) {
            Ok(m) if m.is_dir() => fs::remove_dir_all(velho),
            _ => fs::remove_file(velho),
        };
        match r {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("{}: {e}", velho.display()));
            }
            _ => podar(velho),
        }
    }
    for (caminho, conteudo) in &plano.arquivos {
        if let Some(pai) = caminho.parent() {
            fs::create_dir_all(pai).map_err(|e| format!("{}: {e}", pai.display()))?;
        }
        if *conteudo == Conteudo::Pasta {
            fs::create_dir_all(caminho).map_err(|e| format!("{}: {e}", caminho.display()))?;
            continue;
        }
        let _ = fs::remove_file(caminho);
        let r = match conteudo {
            Conteudo::Pasta => unreachable!("tratada acima"),
            Conteudo::Texto(t) => fs::write(caminho, t),
            Conteudo::Executavel(t) => fs::write(caminho, t)
                .and_then(|_| fs::set_permissions(caminho, fs::Permissions::from_mode(0o755))),
            Conteudo::Link(destino) => std::os::unix::fs::symlink(destino, caminho),
        };
        r.map_err(|e| format!("{}: {e}", caminho.display()))?;
    }
    fs::create_dir_all(dados).map_err(|e| format!("{}: {e}", dados.display()))?;
    let lista: String = novos.iter().map(|p| format!("{}\n", p.display())).collect();
    fs::write(arquivo_registro(dados), lista).map_err(|e| e.to_string())?;

    for (app, entrada) in permissoes_anteriores(dados) {
        if !plano.permissoes.contains(&(app.clone(), entrada.clone())) {
            permissao::revogar_de(home, dados, &app, &entrada)?;
        }
    }
    for (app, entrada) in &plano.permissoes {
        permissao::conceder_a(home, dados, app, entrada)?;
    }
    let texto: String = plano
        .permissoes
        .iter()
        .map(|(a, e)| format!("{a}\t{}\t{}\n", e.chave, e.valor))
        .collect();
    fs::write(arquivo_permissoes(dados), texto).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Home(PathBuf);
    impl Drop for Home {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn home(nome: &str) -> Home {
        let p = std::env::temp_dir().join(format!("ancora-ass-{}-{nome}", std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Home(p)
    }

    const FIREFOX_JSON: &str = r#"{"name":"com.lacunasoftware.webpki","description":"Web PKI","path":"/opt/lacuna-webpki/webpki","type":"stdio","allowed_extensions":["webpki@lacunasoftware.com"]}"#;
    const CHROMIUM_JSON: &str = r#"{"name":"com.lacunasoftware.webpki","description":"Web PKI","path":"/opt/lacuna-webpki/webpki","type":"stdio","allowed_origins":["chrome-extension://dcngeagmmhegagicpcmpinaoklddcgon/"]}"#;

    fn instalado(h: &Path) -> Instalado {
        let dentro = h.join("ext/WebPKI");
        fs::create_dir_all(dentro.join("nm")).unwrap();
        fs::write(dentro.join("nm/firefox.json"), FIREFOX_JSON).unwrap();
        fs::write(dentro.join("nm/chromium.json"), CHROMIUM_JSON).unwrap();
        Instalado {
            id: "WebPKI".into(),
            assinador: Assinador {
                host: "com.lacunasoftware.webpki".into(),
                executavel: dentro.join("extra/bin/webpki"),
                manifesto_firefox: Some(dentro.join("nm/firefox.json")),
                manifesto_chromium: Some(dentro.join("nm/chromium.json")),
                ambiente: BTreeMap::new(),
                pkcs11: vec!["libaetpkss.so.3".into()],
                x11: true,
            },
            dentro,
            raiz_no_host: "/var/lib/flatpak/runtime/dev.lukakuuhaku.Ancora.Assinador.WebPKI".into(),
            arquivos_no_host: "/var/lib/flatpak/runtime/dev.lukakuuhaku.Ancora.Assinador.WebPKI/x86_64/master/active/files".into(),
        }
    }

    fn navegador(nome: &str, flatpak: bool) -> Navegador {
        *NAVEGADORES
            .iter()
            .find(|n| n.nome == nome && n.flatpak.is_some() == flatpak)
            .unwrap()
    }

    #[test]
    fn o_manifesto_troca_so_o_path_e_recusa_a_familia_errada() {
        let m = manifesto(CHROMIUM_JSON, Tipo::Chromium, Path::new("/l/x")).unwrap();
        let v: serde_json::Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["path"], "/l/x");
        assert_eq!(
            v["allowed_origins"][0],
            "chrome-extension://dcngeagmmhegagicpcmpinaoklddcgon/"
        );
        assert_eq!(v["name"], "com.lacunasoftware.webpki");
        assert!(manifesto(CHROMIUM_JSON, Tipo::Firefox, Path::new("/l/x")).is_err());
        assert!(manifesto(FIREFOX_JSON, Tipo::Chromium, Path::new("/l/x")).is_err());
    }

    #[test]
    fn o_lugar_da_extensao_usa_o_link_active() {
        let (raiz, arquivos) =
            lugar("/home/p/.local/share/flatpak/runtime/dev.x.A/x86_64/master/abc123").unwrap();
        assert_eq!(
            raiz,
            PathBuf::from("/home/p/.local/share/flatpak/runtime/dev.x.A")
        );
        assert_eq!(
            arquivos,
            PathBuf::from(
                "/home/p/.local/share/flatpak/runtime/dev.x.A/x86_64/master/active/files"
            )
        );
    }

    #[test]
    fn navegador_flatpak_roda_o_assinador_no_proprio_sandbox() {
        let h = home("flatpak");
        let i = instalado(&h.0);
        let plano = planejar(&h.0, &[navegador("Brave", true)], &[i]).unwrap();
        let base = host::var_app(&h.0, "com.brave.Browser");
        let lancador = base.join("data/ancora/com.lacunasoftware.webpki");

        let achar = |p: &Path| {
            plano
                .arquivos
                .iter()
                .find(|(c, _)| c == p)
                .map(|(_, x)| x.clone())
        };
        assert_eq!(
            achar(&base.join("data/ancora/lib/libaetpkss.so.3")),
            Some(Conteudo::Link(nss::CLIENTE_NO_SANDBOX.into()))
        );
        let Some(Conteudo::Executavel(script)) = achar(&lancador) else {
            panic!("sem lançador")
        };
        assert!(script.contains("exec '/var/lib/flatpak/runtime/dev.lukakuuhaku.Ancora.Assinador.WebPKI/x86_64/master/active/files/extra/bin/webpki' \"$@\""));
        assert!(script.contains("data/ancora/lib'"));
        let Some(Conteudo::Texto(json)) =
            achar(&base.join("config/BraveSoftware/Brave-Browser/NativeMessagingHosts/com.lacunasoftware.webpki.json"))
        else {
            panic!("sem manifesto")
        };
        assert!(json.contains(&format!("\"path\": \"{}\"", lancador.display())));
        assert!(plano.permissoes.contains(&(
            "com.brave.Browser".into(),
            Entrada::arquivos(
                "/var/lib/flatpak/runtime/dev.lukakuuhaku.Ancora.Assinador.WebPKI:ro"
            )
        )));
        assert!(
            plano
                .permissoes
                .contains(&("com.brave.Browser".into(), Entrada::socket("x11")))
        );
        assert!(plano.sem_tokens.contains("com.brave.Browser"));
    }

    #[test]
    fn navegador_nativo_chama_o_ancora_e_firefox_ganha_os_dois_lugares() {
        let h = home("nativo");
        let plano = planejar(&h.0, &[navegador("Firefox", false)], &[instalado(&h.0)]).unwrap();
        let caminhos: Vec<_> = plano
            .arquivos
            .iter()
            .map(|(p, _)| p.strip_prefix(&h.0).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            caminhos,
            vec![
                PathBuf::from(
                    ".config/mozilla/native-messaging-hosts/com.lacunasoftware.webpki.json"
                ),
                PathBuf::from(".local/share/ancora/assinadores/com.lacunasoftware.webpki"),
                PathBuf::from(".mozilla/native-messaging-hosts/com.lacunasoftware.webpki.json"),
            ]
        );
        assert!(plano.permissoes.is_empty());
        assert!(lancador_nativo("com.lacunasoftware.webpki")
            .contains("flatpak run --command=ancora dev.lukakuuhaku.Ancora assinar 'com.lacunasoftware.webpki' \"$@\""));
    }

    #[test]
    fn publicar_e_despublicar_sao_simetricos() {
        let h = home("simetria");
        let dados = h.0.join("dados");
        fs::create_dir_all(host::var_app(&h.0, "org.mozilla.firefox")).unwrap();
        let navegadores = [navegador("Firefox", true), navegador("Chromium", false)];
        let plano = planejar(&h.0, &navegadores, &[instalado(&h.0)]).unwrap();
        aplicar(&h.0, &dados, &plano).unwrap();
        for (p, c) in &plano.arquivos {
            match c {
                Conteudo::Link(_) => assert!(fs::symlink_metadata(p).is_ok(), "{}", p.display()),
                Conteudo::Pasta => assert!(p.is_dir(), "{}", p.display()),
                _ => assert!(p.is_file(), "{}", p.display()),
            }
        }
        // O assinador guarda algo na casa dele.
        let casa = host::var_app(&h.0, "org.mozilla.firefox")
            .join("data/ancora/casa/com.lacunasoftware.webpki");
        fs::write(casa.join("config.json"), "{}").unwrap();
        assert!(permissao::tem_em(
            &h.0,
            "org.mozilla.firefox",
            &Entrada::socket("x11")
        ));

        // De novo não muda nada; vazio tira tudo, inclusive as permissões.
        aplicar(&h.0, &dados, &plano).unwrap();
        aplicar(&h.0, &dados, &Plano::default()).unwrap();
        for (p, _) in &plano.arquivos {
            assert!(fs::symlink_metadata(p).is_err(), "sobrou {}", p.display());
        }
        // As pastas do Âncora saem; a do navegador fica.
        let ff = host::var_app(&h.0, "org.mozilla.firefox");
        assert!(!ff.join("data/ancora").exists());
        assert!(!ff.join(".mozilla/native-messaging-hosts").exists());
        assert!(ff.is_dir());
        assert!(!permissao::tem_em(
            &h.0,
            "org.mozilla.firefox",
            &Entrada::socket("x11")
        ));
    }

    #[test]
    fn so_conta_navegador_presente() {
        let h = home("presentes");
        fs::create_dir_all(h.0.join(".config/BraveSoftware/Brave-Browser")).unwrap();
        fs::create_dir_all(host::var_app(&h.0, "org.mozilla.firefox")).unwrap();
        let flatpaks: BTreeSet<String> = [
            "org.mozilla.firefox".to_owned(),
            "com.google.Chrome".to_owned(),
        ]
        .into();
        let nomes: Vec<_> = presentes(&h.0, &flatpaks)
            .iter()
            .map(|n| (n.nome, n.flatpak.is_some()))
            .collect();
        assert_eq!(nomes, vec![("Brave", false), ("Firefox", true)]);
    }
}
