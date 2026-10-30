//! Os subcomandos de linha de comando: diagnóstico e o que a interface
//! também faz, para testar e para quem prefere o terminal.

use crate::verificacao::{self, Medidor, Resultado};
use crate::{APP_ID, aplicativos, extensao, host, nativos, sonda};
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

/// Os dados do Âncora no host (o registro das permissões concedidas).
fn dados() -> std::path::PathBuf {
    glib::user_data_dir().join("ancora")
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

/// O sentinela publicado para medir, ou, se não der (fora do Flatpak), um
/// medidor que só diz por quê.
fn medidor(home: &Path) -> Box<dyn Medidor> {
    match verificacao::Sentinela::publicar(home) {
        Ok(s) => Box::new(s),
        Err(e) => Box::new(verificacao::Indisponivel(e)),
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
