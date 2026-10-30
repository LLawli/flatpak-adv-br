use adw::prelude::*;
use std::path::Path;

mod extensao;
mod publicacao;
mod servico;
mod sonda;
mod supervisao;

/// O id do aplicativo. É também o prefixo das extensões
/// (`dev.lukakuuhaku.Ancora.Driver.SafeSign`), e por isso não muda.
pub const APP_ID: &str = "dev.lukakuuhaku.Ancora";

const NOME: &str = "Âncora";

fn main() -> gtk::glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("extensoes") => return listar_extensoes(),
        Some("servir") => return servico::servir(),
        Some("sondar") => return sondar(args.get(2).map(Path::new)),
        _ => {}
    }
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(montar_janela);
    app.run()
}

fn montar_janela(app: &adw::Application) {
    let pagina = adw::StatusPage::builder()
        .icon_name(APP_ID)
        .title(NOME)
        .build();

    let conteudo = adw::ToolbarView::new();
    conteudo.add_top_bar(&adw::HeaderBar::new());
    conteudo.set_content(Some(&pagina));

    adw::ApplicationWindow::builder()
        .application(app)
        .title(NOME)
        .default_width(720)
        .default_height(640)
        .content(&conteudo)
        .build()
        .present();
}

/// `ancora extensoes`: o que está montado, uma linha por extensão, pelo ref
/// que o `flatpak` entende. Sai com erro se alguma estiver quebrada.
fn listar_extensoes() -> gtk::glib::ExitCode {
    let home = gtk::glib::home_dir();
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
        gtk::glib::ExitCode::FAILURE
    } else {
        gtk::glib::ExitCode::SUCCESS
    }
}

/// `ancora sondar <socket>`: os rótulos dos tokens, um por linha.
fn sondar(socket: Option<&Path>) -> gtk::glib::ExitCode {
    let Some(socket) = socket else {
        eprintln!("uso: ancora sondar <socket>");
        return gtk::glib::ExitCode::FAILURE;
    };
    match sonda::sondar(socket) {
        Ok(tokens) => {
            for t in tokens {
                println!("{t}");
            }
            gtk::glib::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            gtk::glib::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::APP_ID;

    // O id aparece em quatro lugares que o compilador não vê. Um id trocado
    // num deles só aparece como ícone faltando ou app que não abre, depois
    // de instalado.
    #[test]
    fn o_id_e_o_mesmo_nos_arquivos_de_dados() {
        let desktop = include_str!("../data/dev.lukakuuhaku.Ancora.desktop");
        let metainfo = include_str!("../data/dev.lukakuuhaku.Ancora.metainfo.xml");
        let manifesto = include_str!("../flatpak/dev.lukakuuhaku.Ancora.yml");

        assert!(desktop.contains(&format!("\nIcon={APP_ID}\n")));
        assert!(metainfo.contains(&format!("<id>{APP_ID}</id>")));
        assert!(metainfo.contains(&format!(
            "<launchable type=\"desktop-id\">{APP_ID}.desktop</launchable>"
        )));
        assert!(manifesto.starts_with(&format!("id: {APP_ID}\n")));
    }

    // Um ponto de extensão com diretório diferente do que o código varre
    // monta a extensão num lugar onde ninguém procura.
    #[test]
    fn cada_familia_tem_ponto_de_extensao_onde_o_codigo_procura() {
        use crate::extensao::{Familia, RAIZ};
        let manifesto = include_str!("../flatpak/dev.lukakuuhaku.Ancora.yml");
        let raiz = RAIZ.strip_prefix("/app/").unwrap();
        for familia in Familia::TODAS {
            let ponto = format!(
                "  {APP_ID}.{}:\n    directory: {raiz}/{}\n",
                familia.ponto(),
                familia.diretorio()
            );
            assert!(manifesto.contains(&ponto), "falta no manifesto:\n{ponto}");
            assert!(manifesto.contains(&format!("/app/{raiz}/{}", familia.diretorio())));
        }
    }
}
