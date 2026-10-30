//! Os subcomandos de linha de comando: diagnóstico e o que a interface
//! também faz, para testar e para quem prefere o terminal.

use crate::estado::dados;
use crate::verificacao::{self, Resultado, medidor};
use crate::{
    APP_ID, aplicativos, assinadores, certificado, extensao, host, loja, nativos, pacote, sonda,
};
use gtk::glib::{self, ExitCode};
use std::path::Path;
use std::time::Duration;

/// `ancora extensoes`: o que está montado, uma linha por extensão, pelo ref
/// que o `flatpak` entende. Sai com erro se alguma estiver quebrada.
pub fn extensoes() -> ExitCode {
    let home = glib::home_dir();
    let mut quebradas = false;
    for e in extensao::encontrar(Path::new(extensao::RAIZ), &home) {
        let referencia = format!("{APP_ID}.{}.{}", e.familia.ponto(), e.id);
        match &e.descritor {
            Ok(d) if d.fabricante.is_empty() => println!("{referencia}: {}", d.nome),
            Ok(d) => println!("{referencia}: {} ({})", d.nome, d.fabricante),
            Err(erro) => {
                quebradas = true;
                println!("{referencia}: {erro}");
            }
        }
    }
    if quebradas {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `ancora preparar [Família.Nome]`: baixa e prepara o pacote do fabricante
/// das extensões montadas que têm `[pacote]` (todas, ou a informada). É o que o
/// serviço roda antes de subir um driver.
pub fn preparar(alvo: Option<&str>) -> ExitCode {
    let home = glib::home_dir();
    let mut falhou = false;
    let mut achou = false;
    for e in extensao::encontrar(Path::new(extensao::RAIZ), &home) {
        let nome = format!("{}.{}", e.familia.ponto(), e.id);
        if alvo.is_some_and(|a| a != nome) {
            continue;
        }
        achou = true;
        let Ok(d) = &e.descritor else { continue };
        let Some(p) = &d.pacote else { continue };
        if pacote::pronto(p) {
            println!("{nome}: pronto");
            continue;
        }
        eprintln!("{nome}: baixando {}", p.url);
        let inicio = std::time::Instant::now();
        match pacote::preparar(p) {
            Ok(()) => println!("{nome}: pronto em {} s", inicio.elapsed().as_secs()),
            Err(erro) => {
                falhou = true;
                eprintln!("{nome}: {erro}");
            }
        }
    }
    if let Some(a) = alvo
        && !achou
    {
        eprintln!("{a}: nenhuma extensão montada com esse nome");
        falhou = true;
    }
    if falhou {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// O id completo a partir do que a pessoa digitou: `Driver.SafeSign` ou
/// `dev.lukakuuhaku.Ancora.Driver.SafeSign`.
fn id_de_extensao(arg: Option<&str>) -> Result<String, ExitCode> {
    let Some(arg) = arg else {
        eprintln!("informe a extensão, como Driver.SafeSign");
        return Err(ExitCode::FAILURE);
    };
    let id = if arg.starts_with(APP_ID) {
        arg.to_owned()
    } else {
        format!("{APP_ID}.{arg}")
    };
    if loja::familia_de(&id).is_none() {
        eprintln!("{arg} não é uma extensão do Âncora");
        return Err(ExitCode::FAILURE);
    }
    Ok(id)
}

/// `ancora disponiveis`: as extensões que os remotes oferecem, com o que já
/// está instalado e o que tem atualização.
pub fn disponiveis() -> ExitCode {
    let (lista, erros) = loja::disponiveis();
    for e in &erros {
        eprintln!("{e}");
    }
    let instaladas = loja::instaladas().unwrap_or_default();
    let atualizacoes = loja::atualizacoes(&instaladas);
    for d in &lista {
        let marca = if atualizacoes.contains(&d.id) {
            " [atualização]"
        } else if instaladas.iter().any(|i| i.id == d.id) {
            " [instalada]"
        } else {
            ""
        };
        println!("{}: {} ({}){marca}", d.id, d.nome, d.remoto);
    }
    if lista.is_empty() && !erros.is_empty() {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn concluir(r: Result<(), String>, feito: &str) -> ExitCode {
    match r {
        Ok(()) => {
            println!("{feito}. Abra o Âncora de novo para valer.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `ancora instalar <extensão>`: do remote que a oferece.
pub fn instalar(arg: Option<&str>) -> ExitCode {
    let id = match id_de_extensao(arg) {
        Ok(id) => id,
        Err(c) => return c,
    };
    let (lista, erros) = loja::disponiveis();
    let Some(d) = lista.into_iter().find(|d| d.id == id) else {
        for e in &erros {
            eprintln!("{e}");
        }
        eprintln!("nenhum remote oferece {id}");
        return ExitCode::FAILURE;
    };
    concluir(loja::instalar(&d), "instalada")
}

fn instalada(id: &str) -> Result<loja::Instalada, ExitCode> {
    match loja::instaladas() {
        Ok(l) => l.into_iter().find(|i| i.id == id).ok_or_else(|| {
            eprintln!("{id} não está instalada");
            ExitCode::FAILURE
        }),
        Err(e) => {
            eprintln!("{e}");
            Err(ExitCode::FAILURE)
        }
    }
}

/// `ancora atualizar <extensão>`.
pub fn atualizar(arg: Option<&str>) -> ExitCode {
    match id_de_extensao(arg).and_then(|id| instalada(&id)) {
        Ok(i) => concluir(loja::atualizar(&i), "atualizada"),
        Err(c) => c,
    }
}

/// `ancora remover <extensão>`.
pub fn remover(arg: Option<&str>) -> ExitCode {
    match id_de_extensao(arg).and_then(|id| instalada(&id)) {
        Ok(i) => concluir(loja::remover(&i), "removida"),
        Err(c) => c,
    }
}

/// `ancora sondar <socket>`: os rótulos dos tokens, um por linha.
pub fn sondar(socket: Option<&Path>) -> ExitCode {
    let Some(socket) = socket else {
        eprintln!("uso: ancora sondar <socket>");
        return ExitCode::FAILURE;
    };
    match sonda::sondar(socket) {
        Ok(tokens) => {
            for t in tokens {
                println!("{t}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `ancora certificados <socket>`: os tokens e os certificados de cada um,
/// em JSON, para a janela.
pub fn certificados(socket: Option<&Path>) -> ExitCode {
    let Some(socket) = socket else {
        eprintln!("uso: ancora certificados <socket>");
        return ExitCode::FAILURE;
    };
    match certificado::listar(socket) {
        Ok(tokens) => {
            println!("{}", serde_json::to_string(&tokens).unwrap_or_default());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `ancora aplicativos`: o servidor do sistema e cada aplicativo conhecido
/// que está instalado, com o que falta nele.
pub fn aplicativos() -> ExitCode {
    let socket = aplicativos::socket_do_sistema();
    match sonda::sondar_com_limite(&socket, Duration::from_secs(10)) {
        Ok(tokens) if tokens.is_empty() => println!("servidor do sistema: sem tokens"),
        Ok(tokens) => println!("servidor do sistema: {}", tokens.join(", ")),
        Err(e) => println!("servidor do sistema: {e}"),
    }
    let instalados = match aplicativos::instalados() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("não consegui listar os aplicativos instalados: {e}");
            return ExitCode::FAILURE;
        }
    };
    let home = host::home();
    for app in aplicativos::CONHECIDOS
        .iter()
        .filter(|a| instalados.contains(a.id))
    {
        let s = aplicativos::situacao(app, &home, &dados());
        if s.pronto() {
            let aviso = if s.banco == aplicativos::Banco::NaoVerificado {
                " (não verificado)"
            } else {
                ""
            };
            println!("{} ({}): pronto{aviso}", app.nome, app.id);
            continue;
        }
        let mut falta = Vec::new();
        if !s.permissao {
            falta.push("permissão".to_owned());
        }
        match s.banco {
            aplicativos::Banco::Falta(o_que) => falta.push(o_que),
            aplicativos::Banco::EmUso => {
                falta.push("feche o aplicativo para acertar o banco".into())
            }
            aplicativos::Banco::Pronto
            | aplicativos::Banco::NaoSeAplica
            | aplicativos::Banco::NaoVerificado => {}
        }
        println!("{} ({}): falta {}", app.nome, app.id, falta.join("; "));
    }
    ExitCode::SUCCESS
}

fn conhecido(id: Option<&str>) -> Result<aplicativos::Aplicativo, ExitCode> {
    let Some(id) = id else {
        eprintln!("informe o id do aplicativo, como org.mozilla.firefox");
        return Err(ExitCode::FAILURE);
    };
    aplicativos::por_id(id).ok_or_else(|| {
        eprintln!("{id} não é um aplicativo que o Âncora conheça");
        ExitCode::FAILURE
    })
}

/// `ancora configurar <id>`: liga o servidor do sistema e faz os passos do
/// aplicativo.
pub fn configurar(id: Option<&str>) -> ExitCode {
    let app = match conhecido(id) {
        Ok(a) => a,
        Err(c) => return c,
    };
    if let Err(e) = aplicativos::ligar_servidor_do_sistema() {
        eprintln!("não consegui ligar o p11-kit-server.socket: {e}");
        return ExitCode::FAILURE;
    }
    let home = host::home();
    let medidor = medidor(&home);
    match aplicativos::configurar(&app, &home, &dados(), medidor.as_ref()) {
        Ok(resultados) => {
            relatar(&resultados);
            println!("{}: pronto. Feche e abra o aplicativo de novo.", app.nome);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{}: {e}", app.nome);
            ExitCode::FAILURE
        }
    }
}

/// `ancora desfazer <id>`: tira do aplicativo o que o Âncora pôs.
pub fn desfazer(id: Option<&str>) -> ExitCode {
    let app = match conhecido(id) {
        Ok(a) => a,
        Err(c) => return c,
    };
    match aplicativos::desfazer(&app, &host::home(), &dados()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e}", app.nome);
            ExitCode::FAILURE
        }
    }
}

/// `ancora nativos`: em que navegadores nativos o Âncora está publicado.
/// `ancora nativos publicar|despublicar <firefox|sistema>`: muda isso.
pub fn nativos(acao: Option<&str>, destino: Option<&str>) -> ExitCode {
    let home = host::home();
    let Some(acao) = acao else {
        for d in nativos::Destino::TODOS {
            let s = nativos::situacao(d, &home, &dados());
            let estado = if s.bancos == 0 {
                "sem banco (abra o navegador uma vez)".to_owned()
            } else if s.publicado() && s.verificados == s.bancos {
                "publicado".to_owned()
            } else if s.publicado() {
                "publicado (não verificado)".to_owned()
            } else {
                format!("não publicado ({} de {} bancos)", s.publicados, s.bancos)
            };
            println!("{} ({}): {estado}", d.nome(), d.id());
        }
        return ExitCode::SUCCESS;
    };
    let Some(d) = destino.and_then(nativos::Destino::por_id) else {
        eprintln!("uso: ancora nativos publicar|despublicar firefox|sistema");
        return ExitCode::FAILURE;
    };
    let resultado = match acao {
        "publicar" => nativos::proxy_do_host().and_then(|p| {
            let medidor = medidor(&home);
            nativos::publicar(d, &home, &dados(), &p, medidor.as_ref()).map(|r| relatar(&r))
        }),
        "despublicar" => nativos::despublicar(d, &home, &dados()),
        _ => Err("uso: ancora nativos publicar|despublicar firefox|sistema".to_owned()),
    };
    match resultado {
        Ok(()) => {
            println!("{}: feito. Feche e abra o navegador de novo.", d.nome());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn relatar(resultados: &[(std::path::PathBuf, Resultado)]) {
    for (banco, r) in resultados {
        let texto = match r {
            Resultado::Uma(None) => "o token aparece uma vez sem registro do Âncora".to_owned(),
            Resultado::Uma(Some(lib)) => format!("o token aparece uma vez, registrado com {lib}"),
            Resultado::NaoVerificado { registro, motivo } => format!(
                "não verificado ({motivo}); ficou {}",
                registro.as_deref().unwrap_or("sem registro")
            ),
        };
        println!("  {}: {texto}", banco.display());
    }
}

/// `ancora assinadores`: os assinadores instalados e os navegadores
/// presentes. `ancora assinadores publicar|despublicar`: muda isso.
pub fn assinadores(acao: Option<&str>) -> ExitCode {
    let home = host::home();
    if acao == Some("despublicar") {
        let r = assinadores::ligar(&dados(), &Default::default())
            .and_then(|()| assinadores::sincronizar(&home, &dados()));
        return match r {
            Ok(_) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{e}");
                ExitCode::FAILURE
            }
        };
    }
    let flatpaks = match aplicativos::instalados() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("não consegui listar os aplicativos instalados: {e}");
            return ExitCode::FAILURE;
        }
    };
    let navegadores = assinadores::presentes(&home, &flatpaks);
    let mut instalados = Vec::new();
    for i in assinadores::instalados() {
        match i {
            Ok(i) => instalados.push(i),
            Err(e) => eprintln!("{e}"),
        }
    }
    for i in &instalados {
        println!("assinador: {} ({})", i.assinador.host, i.id);
    }
    for n in &navegadores {
        match n.flatpak {
            Some(id) => println!("navegador: {} ({id})", n.nome),
            None => println!("navegador: {} (nativo)", n.nome),
        }
    }
    if acao != Some("publicar") {
        return ExitCode::SUCCESS;
    }
    let todos = instalados.iter().map(|i| i.id.clone()).collect();
    let resultado = assinadores::ligar(&dados(), &todos)
        .and_then(|()| assinadores::sincronizar(&home, &dados()));
    match resultado {
        Ok(plano) => {
            for id in &plano.sem_tokens {
                println!(
                    "{id}: sem a permissão do p11-kit; rode `ancora configurar {id}` para os tokens aparecerem no assinador"
                );
            }
            println!("publicado. Feche e abra os navegadores de novo.");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// `ancora assinar <host> [argumentos]`: o que o manifesto de um navegador
/// nativo executa (pelo `flatpak run`). Roda o assinador neste sandbox, com
/// os nomes de biblioteca que a extensão pede apontando para o cliente do
/// p11-kit. A saída padrão é o protocolo do native messaging: nada é escrito
/// nela antes do assinador.
pub fn assinar(host_nm: Option<&str>, argumentos: &[String]) -> ExitCode {
    use std::os::unix::process::CommandExt;
    let Some(host_nm) = host_nm else {
        eprintln!("uso: ancora assinar <host> [argumentos]");
        return ExitCode::FAILURE;
    };
    let home = glib::home_dir();
    let achado = extensao::encontrar(Path::new(extensao::RAIZ), &home)
        .into_iter()
        .filter_map(|e| e.descritor.ok())
        .find_map(|d| {
            let a = d.assinadores.into_iter().find(|a| a.host == host_nm)?;
            Some((d.pacote, a))
        });
    let Some((pacote, a)) = achado else {
        eprintln!("nenhum assinador instalado responde por {host_nm}");
        return ExitCode::FAILURE;
    };
    // Normalmente já preparado ao publicar; aqui só se a pasta sumiu.
    if let Some(p) = &pacote
        && let Err(e) = pacote::preparar(p)
    {
        eprintln!("{host_nm}: {e}");
        return ExitCode::FAILURE;
    }
    // No $XDG_RUNTIME_DIR do sandbox, que é de cada instância: o /tmp de um
    // aplicativo Flatpak é compartilhado entre as instâncias e sobrevive a
    // elas, e os PIDs dentro do sandbox se repetem.
    let bibliotecas = glib::user_runtime_dir().join("ancora-assinador");
    let casa = glib::user_data_dir().join("assinadores").join(host_nm);
    let preparo = (|| -> std::io::Result<()> {
        std::fs::create_dir_all(&bibliotecas)?;
        std::fs::create_dir_all(&casa)?;
        for nome in &a.pkcs11 {
            let link = bibliotecas.join(nome);
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(sonda::cliente(), link)?;
        }
        Ok(())
    })();
    if let Err(e) = preparo {
        eprintln!("{host_nm}: {e}");
        return ExitCode::FAILURE;
    }
    let mut caminho = std::ffi::OsString::from(bibliotecas.as_os_str());
    if let Some(atual) = std::env::var_os("LD_LIBRARY_PATH") {
        caminho.push(":");
        caminho.push(atual);
    }
    let mut cmd = std::process::Command::new(&a.executavel);
    cmd.args(argumentos)
        .env("LD_LIBRARY_PATH", caminho)
        .env("HOME", &casa)
        .envs(&a.ambiente);
    // Só nos testes: o servidor do sistema da sessão serviria os tokens reais.
    if let Some(s) = std::env::var_os(verificacao::VARIAVEL_SERVIDOR) {
        let mut endereco = std::ffi::OsString::from("unix:path=");
        endereco.push(s);
        cmd.env("P11_KIT_SERVER_ADDRESS", endereco);
    }
    let erro = cmd.exec();
    eprintln!("{}: {erro}", a.executavel.display());
    ExitCode::FAILURE
}
