//! As extensões nos remotes que a pessoa já tem: o que dá para instalar, o que
//! tem atualização, e instalar, atualizar e remover, pelo `flatpak` do host.
//!
//! O Flatpak monta as extensões quando o sandbox começa. O que muda no host
//! (instalar, atualizar, remover, aqui ou por fora) só vale num Âncora aberto
//! de novo, e a diferença entre o que está montado e o que está instalado é o
//! que a janela mostra como reinício pendente.
//!
//! O `flatpak` roda com `LC_ALL=C`: as chaves do `--json` são traduzidas
//! conforme o idioma (`id_de_aplicativo` em português).

use crate::APP_ID;
use crate::extensao::Familia;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Uma extensão oferecida por um remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disponivel {
    pub id: String,
    pub familia: Familia,
    pub ramo: String,
    pub remoto: String,
    /// Onde o remote está configurado, e onde a extensão vai ser instalada.
    pub instalacao: Instalacao,
    /// O nome do appstream; sem ele, o Flatpak dá o último pedaço do id.
    pub nome: String,
    pub descricao: String,
}

/// Uma extensão instalada no host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instalada {
    pub id: String,
    pub ramo: String,
    pub remoto: String,
    pub instalacao: Instalacao,
    /// O commit ativo, abreviado como o `flatpak list` o dá.
    pub commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Instalacao {
    Usuario,
    Sistema,
    /// Uma instalação a mais, configurada em `/etc/flatpak/installations.d`.
    Outra(String),
}

impl Instalacao {
    fn de(nome: &str) -> Instalacao {
        match nome {
            "user" => Instalacao::Usuario,
            "system" => Instalacao::Sistema,
            outra => Instalacao::Outra(outra.to_owned()),
        }
    }

    fn opcao(&self) -> String {
        match self {
            Instalacao::Usuario => "--user".into(),
            Instalacao::Sistema => "--system".into(),
            Instalacao::Outra(n) => format!("--installation={n}"),
        }
    }
}

/// A família e o nome de um id de extensão do Âncora
/// (`dev.lukakuuhaku.Ancora.Driver.SafeSign`). Os refs que o Flatpak gera ao
/// lado (`.Debug`, `.Locale`) e o próprio aplicativo ficam de fora.
pub fn familia_de(id: &str) -> Option<(Familia, &str)> {
    let resto = id.strip_prefix(APP_ID)?.strip_prefix('.')?;
    let (ponto, nome) = resto.split_once('.')?;
    let familia = Familia::TODAS.into_iter().find(|f| f.ponto() == ponto)?;
    (!nome.is_empty() && !nome.contains('.')).then_some((familia, nome))
}

/// O `flatpak` do host, com a saída em inglês.
fn flatpak(argumentos: &[&str]) -> Result<String, String> {
    let saida = Command::new("flatpak-spawn")
        .args(["--host", "env", "LC_ALL=C", "flatpak"])
        .args(argumentos)
        .output()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    if !saida.status.success() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        return Err(erro.trim().trim_start_matches("error: ").to_owned());
    }
    Ok(String::from_utf8_lossy(&saida.stdout).into_owned())
}

/// Os remotes ligados e onde cada um está. Os `no-enumerate` (os que o
/// flatpak-builder cria para cada instalação local) não oferecem nada.
fn remotos(texto: &str) -> Vec<(String, Instalacao)> {
    texto
        .lines()
        .filter_map(|l| {
            let (nome, opcoes) = l.split_once('\t')?;
            let opcoes: Vec<&str> = opcoes.trim().split(',').collect();
            if opcoes
                .iter()
                .any(|o| *o == "disabled" || *o == "no-enumerate")
            {
                return None;
            }
            Some((nome.trim().to_owned(), Instalacao::de(opcoes.first()?)))
        })
        .collect()
}

/// O `--json` do `flatpak` não imprime nada, nem `[]`, quando a lista é
/// vazia.
fn json<T: serde::de::DeserializeOwned>(texto: &str) -> Result<Vec<T>, String> {
    if texto.trim().is_empty() {
        return Ok(vec![]);
    }
    serde_json::from_str(texto).map_err(|e| e.to_string())
}

#[derive(Deserialize)]
struct LinhaRemoto {
    application_id: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    description: String,
}

fn ofertas(texto: &str, remoto: &str, instalacao: &Instalacao) -> Result<Vec<Disponivel>, String> {
    let linhas: Vec<LinhaRemoto> = json(texto)?;
    Ok(linhas
        .into_iter()
        .filter_map(|l| {
            let (familia, curto) = familia_de(&l.application_id)?;
            Some(Disponivel {
                nome: if l.name.is_empty() {
                    curto.to_owned()
                } else {
                    l.name
                },
                familia,
                ramo: l.branch,
                remoto: remoto.to_owned(),
                instalacao: instalacao.clone(),
                descricao: l.description,
                id: l.application_id,
            })
        })
        .collect())
}

/// As extensões que os remotes oferecem, uma por id: a do remote do usuário
/// ganha, por instalar sem pedir senha. Um remote fora do ar não esconde os
/// outros; o erro dele vem à parte.
pub fn disponiveis() -> (Vec<Disponivel>, Vec<String>) {
    let lista = match flatpak(&["remotes", "--columns=name,options"]) {
        Ok(t) => remotos(&t),
        Err(e) => return (vec![], vec![e]),
    };
    let mut todas = Vec::new();
    let mut erros = Vec::new();
    for (remoto, instalacao) in lista {
        let listar = || {
            flatpak(&[
                "remote-ls",
                &instalacao.opcao(),
                &remoto,
                "--runtime",
                "--json",
                "--columns=application,branch,name,description",
            ])
            .and_then(|json| ofertas(&json, &remoto, &instalacao))
        };
        let mut r = listar();
        // O nome e a descrição vêm do appstream que o cliente guardou do
        // remote, e um remote recém-adicionado (ou uma extensão nova nele)
        // ainda não está lá. Atualizar só nesse caso: o do Flathub levou
        // 133 s, medido em 2026-10-30.
        if r.as_ref().is_ok_and(|o| o.iter().any(sem_appstream)) {
            let _ = flatpak(&["update", &instalacao.opcao(), "--appstream", &remoto]);
            r = listar();
        }
        match r {
            Ok(o) => todas.extend(o),
            Err(e) => erros.push(format!("{remoto}: {e}")),
        }
    }
    (uma_por_id(todas), erros)
}

/// Sem appstream, o Flatpak dá como nome o último pedaço do id.
fn sem_appstream(d: &Disponivel) -> bool {
    familia_de(&d.id).is_none_or(|(_, curto)| d.nome == curto)
}

fn uma_por_id(mut todas: Vec<Disponivel>) -> Vec<Disponivel> {
    todas.sort_by(|a, b| {
        (a.familia, &a.nome, &a.id, &a.instalacao).cmp(&(b.familia, &b.nome, &b.id, &b.instalacao))
    });
    let mut vistos = BTreeSet::new();
    todas.retain(|d| vistos.insert(d.id.clone()));
    todas
}

#[derive(Deserialize)]
struct LinhaInstalada {
    application_id: String,
    #[serde(default)]
    branch: String,
    #[serde(default)]
    origin: String,
    #[serde(default)]
    installation: String,
    /// Abreviado: os 12 primeiros caracteres.
    #[serde(default)]
    active_commit: String,
}

fn lidas(texto: &str) -> Result<Vec<Instalada>, String> {
    let linhas: Vec<LinhaInstalada> = json(texto)?;
    Ok(linhas
        .into_iter()
        .filter(|l| familia_de(&l.application_id).is_some())
        .map(|l| Instalada {
            id: l.application_id,
            ramo: l.branch,
            remoto: l.origin,
            instalacao: Instalacao::de(&l.installation),
            commit: l.active_commit,
        })
        .collect())
}

/// As extensões do Âncora instaladas no host, em qualquer instalação.
pub fn instaladas() -> Result<Vec<Instalada>, String> {
    let json = flatpak(&[
        "list",
        "--runtime",
        "--json",
        "--columns=application,branch,origin,installation,active",
    ])?;
    lidas(&json)
}

#[derive(Deserialize)]
struct LinhaAtualizacao {
    application_id: String,
}

/// As extensões instaladas com versão nova no remote. Pergunta à rede: sem
/// ela, não há atualização a mostrar, e isso não é erro.
pub fn atualizacoes(instaladas: &[Instalada]) -> BTreeSet<String> {
    let instalacoes: BTreeSet<&Instalacao> = instaladas.iter().map(|i| &i.instalacao).collect();
    let mut ids = BTreeSet::new();
    for instalacao in instalacoes {
        let Ok(saida) = flatpak(&[
            "remote-ls",
            &instalacao.opcao(),
            "--updates",
            "--runtime",
            "--json",
            "--columns=application",
        ]) else {
            continue;
        };
        let linhas: Vec<LinhaAtualizacao> = json(&saida).unwrap_or_default();
        ids.extend(
            linhas
                .into_iter()
                .map(|l| l.application_id)
                .filter(|id| familia_de(id).is_some()),
        );
    }
    ids
}

fn referencia(id: &str, ramo: &str) -> String {
    if ramo.is_empty() {
        id.to_owned()
    } else {
        format!("{id}//{ramo}")
    }
}

/// Instala. As extensões de fabricante usam `extra-data`: o pacote é baixado
/// do site dele agora, na máquina de quem instala.
pub fn instalar(d: &Disponivel) -> Result<(), String> {
    flatpak(&[
        "install",
        &d.instalacao.opcao(),
        "--noninteractive",
        "-y",
        &d.remoto,
        &referencia(&d.id, &d.ramo),
    ])
    .map(drop)
}

pub fn atualizar(i: &Instalada) -> Result<(), String> {
    flatpak(&[
        "update",
        &i.instalacao.opcao(),
        "--noninteractive",
        "-y",
        &referencia(&i.id, &i.ramo),
    ])
    .map(drop)
}

pub fn remover(i: &Instalada) -> Result<(), String> {
    flatpak(&[
        "uninstall",
        &i.instalacao.opcao(),
        "--noninteractive",
        "-y",
        &referencia(&i.id, &i.ramo),
    ])
    .map(drop)
}

/// As extensões montadas neste sandbox, com o commit de cada uma, lidas do
/// `/.flatpak-info` (`app-extensions=id=commit;...`).
pub fn montadas(flatpak_info: &str) -> BTreeMap<String, String> {
    let mut na_instancia = false;
    for linha in flatpak_info.lines() {
        let linha = linha.trim();
        if linha.starts_with('[') {
            na_instancia = linha == "[Instance]";
            continue;
        }
        if na_instancia && let Some(valor) = linha.strip_prefix("app-extensions=") {
            return valor
                .split(';')
                .filter_map(|e| e.split_once('='))
                .filter(|(id, _)| familia_de(id).is_some())
                .map(|(id, commit)| (id.to_owned(), commit.to_owned()))
                .collect();
        }
    }
    BTreeMap::new()
}

/// Se o que está instalado no host difere do que este sandbox montou: uma
/// extensão a mais, a menos, ou num commit diferente.
/// O commit do host vem abreviado, e o montado, inteiro.
pub fn reinicio_pendente(montadas: &BTreeMap<String, String>, instaladas: &[Instalada]) -> bool {
    let no_host: BTreeMap<&str, &str> = instaladas
        .iter()
        .map(|i| (i.id.as_str(), i.commit.as_str()))
        .collect();
    if no_host.len() != montadas.len() {
        return true;
    }
    montadas.iter().any(|(id, inteiro)| {
        no_host
            .get(id.as_str())
            .is_none_or(|curto| curto.is_empty() || !inteiro.starts_with(curto))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconhece_so_as_extensoes_do_ancora() {
        assert_eq!(
            familia_de("dev.lukakuuhaku.Ancora.Driver.SafeSign"),
            Some((Familia::Driver, "SafeSign"))
        );
        assert_eq!(
            familia_de("dev.lukakuuhaku.Ancora.Assinador.WebPKI"),
            Some((Familia::Assinador, "WebPKI"))
        );
        assert_eq!(
            familia_de("dev.lukakuuhaku.Ancora.App.PJeOffice"),
            Some((Familia::App, "PJeOffice"))
        );
        for fora in [
            "dev.lukakuuhaku.Ancora",
            "dev.lukakuuhaku.Ancora.Debug",
            "dev.lukakuuhaku.Ancora.Driver.SafeSign.Debug",
            "dev.lukakuuhaku.Ancora.Driver.",
            "dev.lukakuuhaku.Ancora.Outra.Coisa",
            "dev.lukakuuhaku.AncoraX.Driver.SafeSign",
            "dev.lukakuuhaku.AdvBr.Driver.SafeSign",
        ] {
            assert_eq!(familia_de(fora), None, "{fora}");
        }
    }

    #[test]
    fn pula_remotes_desligados_e_os_do_builder() {
        let texto = "flathub\tsystem\nadv-br\tuser\nancora-origin\tuser,no-enumerate,no-gpg-verify\nvelho\tsystem,disabled\nempresa\tlab,no-gpg-verify\n";
        assert_eq!(
            remotos(texto),
            vec![
                ("flathub".to_owned(), Instalacao::Sistema),
                ("adv-br".to_owned(), Instalacao::Usuario),
                ("empresa".to_owned(), Instalacao::Outra("lab".into())),
            ]
        );
        assert_eq!(
            Instalacao::Outra("lab".into()).opcao(),
            "--installation=lab"
        );
    }

    // A saída real do `flatpak remote-ls --json` (1.18.2) com LC_ALL=C.
    const REMOTE_LS: &str = r#"[
  {
    "application_id" : "dev.lukakuuhaku.Ancora.App.Teste",
    "branch" : "master",
    "name" : "Teste",
    "description" : ""
  },
  {
    "application_id" : "dev.lukakuuhaku.Ancora.Driver.TesteA3",
    "branch" : "master",
    "name" : "Token de teste",
    "description" : "Token em software, para testar"
  },
  {
    "application_id" : "dev.lukakuuhaku.Ancora.Driver.TesteA3.Debug",
    "branch" : "master",
    "name" : "Debug",
    "description" : ""
  }
]"#;

    #[test]
    fn le_o_que_o_remote_oferece() {
        let o = ofertas(REMOTE_LS, "ancora-teste", &Instalacao::Usuario).unwrap();
        assert_eq!(o.len(), 2);
        assert!(sem_appstream(&o[0]));
        assert!(!sem_appstream(&o[1]));
        assert_eq!(o[1].nome, "Token de teste");
        assert_eq!(o[1].familia, Familia::Driver);
        assert_eq!(o[1].descricao, "Token em software, para testar");
        assert_eq!(o[1].remoto, "ancora-teste");
        assert!(ofertas("[]", "x", &Instalacao::Usuario).unwrap().is_empty());
        assert!(ofertas("\n", "x", &Instalacao::Usuario).unwrap().is_empty());
        assert!(ofertas("lixo", "x", &Instalacao::Usuario).is_err());
    }

    #[test]
    fn a_mesma_extensao_em_dois_remotes_aparece_uma_vez_pelo_do_usuario() {
        let d = |instalacao, remoto: &str| Disponivel {
            id: "dev.lukakuuhaku.Ancora.Driver.SafeSign".into(),
            familia: Familia::Driver,
            ramo: "master".into(),
            remoto: remoto.into(),
            instalacao,
            nome: "SafeSign".into(),
            descricao: String::new(),
        };
        let r = uma_por_id(vec![
            d(Instalacao::Sistema, "flathub"),
            d(Instalacao::Usuario, "ancora"),
        ]);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].remoto, "ancora");
    }

    #[test]
    fn le_as_instaladas_e_compara_com_as_montadas() {
        let json = r#"[
          {"application_id": "org.gnome.Platform", "branch": "51", "origin": "flathub", "installation": "system", "active_commit": "aaa"},
          {"application_id": "dev.lukakuuhaku.Ancora.Debug", "branch": "master", "origin": "ancora-origin", "installation": "user", "active_commit": "bbb"},
          {"application_id": "dev.lukakuuhaku.Ancora.Driver.TesteA3", "branch": "master", "origin": "ancora-teste", "installation": "user", "active_commit": "c1ab"}
        ]"#;
        let i = lidas(json).unwrap();
        assert_eq!(i.len(), 1);
        assert_eq!(i[0].instalacao, Instalacao::Usuario);
        assert_eq!(i[0].remoto, "ancora-teste");

        let info = "[Application]\nname=dev.lukakuuhaku.Ancora\n\n[Instance]\ninstance-id=184569770\napp-extensions=dev.lukakuuhaku.Ancora.Debug=bbb;dev.lukakuuhaku.Ancora.Driver.TesteA3=c1ab99\nbranch=master\n";
        let m = montadas(info);
        assert_eq!(
            m,
            BTreeMap::from([(
                "dev.lukakuuhaku.Ancora.Driver.TesteA3".into(),
                "c1ab99".into()
            )])
        );
        assert!(!reinicio_pendente(&m, &i));

        // Atualizada no host: outro commit.
        let mut nova = i.clone();
        nova[0].commit = "c2ab".into();
        assert!(reinicio_pendente(&m, &nova));
        // Removida no host, e instalada sem estar montada.
        assert!(reinicio_pendente(&m, &[]));
        assert!(reinicio_pendente(&BTreeMap::new(), &i));
        // Sem extensões dos dois lados.
        assert!(!reinicio_pendente(&montadas("[Instance]\n"), &[]));
    }

    #[test]
    fn a_referencia_leva_o_ramo_quando_ha() {
        assert_eq!(referencia("a.b.C", "master"), "a.b.C//master");
        assert_eq!(referencia("a.b.C", ""), "a.b.C");
    }
}
