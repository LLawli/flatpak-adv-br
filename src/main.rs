use adw::prelude::*;

/// O id do aplicativo. É também o prefixo das extensões
/// (`dev.lukakuuhaku.Ancora.Driver.SafeSign`), e por isso não muda.
pub const APP_ID: &str = "dev.lukakuuhaku.Ancora";

const NOME: &str = "Âncora";

fn main() -> gtk::glib::ExitCode {
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
}
