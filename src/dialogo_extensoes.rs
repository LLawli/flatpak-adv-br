//! O diálogo de adicionar extensões: o que os remotes oferecem, por família,
//! com busca, e instalar com um clique.
//!
//! A lista vem dos remotes que a pessoa já tem (ver `loja`), lida fora da
//! thread da interface ao abrir. Instalar não muda este sandbox: a extensão
//! vale quando o Âncora abre de novo, e a janela principal mostra isso.

use crate::extensao::Familia;
use crate::loja::{self, Disponivel};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

fn titulo_da_familia(f: Familia) -> &'static str {
    match f {
        Familia::Driver => "Drivers",
        Familia::Assinador => "Assinadores",
        Familia::App => "Aplicativos",
    }
}

/// O texto em que a busca procura: nome, descrição e id, sem maiúsculas.
fn texto_de_busca(d: &Disponivel) -> String {
    format!("{} {} {}", d.nome, d.descricao, d.id).to_lowercase()
}

/// Se a linha aparece para o que foi digitado: cada palavra precisa estar no
/// texto, em qualquer ordem.
fn combina(texto: &str, busca: &str) -> bool {
    busca
        .to_lowercase()
        .split_whitespace()
        .all(|p| texto.contains(p))
}

struct Linha {
    linha: adw::ActionRow,
    grupo: adw::PreferencesGroup,
    texto: String,
}

struct Dialogo {
    dialogo: adw::Dialog,
    avisos: adw::ToastOverlay,
    pilha: gtk::Stack,
    pagina: adw::PreferencesPage,
    vazio: adw::StatusPage,
    linhas: RefCell<Vec<Linha>>,
    grupos: RefCell<Vec<adw::PreferencesGroup>>,
    /// Chamado depois de cada instalação, para a janela principal se refazer.
    instalou: Box<dyn Fn()>,
}

pub fn abrir(pai: &impl IsA<gtk::Widget>, instalou: impl Fn() + 'static) {
    let busca = gtk::SearchEntry::builder()
        .placeholder_text("Buscar")
        .hexpand(true)
        .build();
    let cabecalho = adw::HeaderBar::new();
    cabecalho.set_title_widget(Some(
        &adw::Clamp::builder()
            .maximum_size(360)
            .child(&busca)
            .build(),
    ));

    let carregando = adw::StatusPage::builder()
        .child(
            &adw::Spinner::builder()
                .width_request(32)
                .height_request(32)
                .build(),
        )
        .build();
    let vazio = adw::StatusPage::builder()
        .icon_name("application-x-addon-symbolic")
        .title("Nenhuma extensão disponível")
        .build();
    let pagina = adw::PreferencesPage::new();
    let pilha = gtk::Stack::new();
    pilha.add_named(&carregando, Some("carregando"));
    pilha.add_named(&vazio, Some("vazio"));
    pilha.add_named(&pagina, Some("lista"));

    let vista = adw::ToolbarView::new();
    vista.add_top_bar(&cabecalho);
    vista.set_content(Some(&pilha));
    let avisos = adw::ToastOverlay::new();
    avisos.set_child(Some(&vista));

    let dialogo = adw::Dialog::builder()
        .title("Adicionar extensões")
        .content_width(560)
        .content_height(640)
        .child(&avisos)
        .build();
    busca.set_key_capture_widget(Some(&dialogo));

    let d = Rc::new(Dialogo {
        dialogo,
        avisos,
        pilha,
        pagina,
        vazio,
        linhas: RefCell::default(),
        grupos: RefCell::default(),
        instalou: Box::new(instalou),
    });
    {
        let d = Rc::downgrade(&d);
        busca.connect_search_changed(move |b| {
            if let Some(d) = d.upgrade() {
                d.filtrar(&b.text());
            }
        });
    }
    {
        // O diálogo guarda a si mesmo até fechar: os botões só têm referências
        // fracas a ele.
        let guarda = RefCell::new(Some(d.clone()));
        d.dialogo.connect_closed(move |_| {
            guarda.borrow_mut().take();
        });
    }
    d.dialogo.present(Some(pai));

    let eu = d.clone();
    glib::spawn_future_local(async move {
        let r = gio::spawn_blocking(|| {
            let (lista, erros) = loja::disponiveis();
            let instaladas: BTreeSet<String> = loja::instaladas()
                .unwrap_or_default()
                .into_iter()
                .map(|i| i.id)
                .collect();
            (lista, erros, instaladas)
        })
        .await;
        let Ok((lista, erros, instaladas)) = r else {
            return;
        };
        eu.mostrar(lista, &erros, &instaladas);
    });
}

impl Dialogo {
    fn avisar(&self, texto: &str) {
        self.avisos.add_toast(adw::Toast::new(texto));
    }

    fn mostrar(
        self: &Rc<Self>,
        lista: Vec<Disponivel>,
        erros: &[String],
        instaladas: &BTreeSet<String>,
    ) {
        if lista.is_empty() {
            if !erros.is_empty() {
                self.vazio.set_icon_name(Some("network-offline-symbolic"));
                self.vazio.set_title("Os remotes não responderam");
                self.vazio.set_description(Some(&erros.join("\n")));
            }
            self.pilha.set_visible_child_name("vazio");
            return;
        }
        for e in erros {
            self.avisar(e);
        }
        for familia in Familia::TODAS {
            let grupo = adw::PreferencesGroup::builder()
                .title(titulo_da_familia(familia))
                .build();
            let mut algum = false;
            for item in lista.iter().filter(|d| d.familia == familia) {
                algum = true;
                let linha = self.linha(item, instaladas.contains(&item.id));
                grupo.add(&linha);
                self.linhas.borrow_mut().push(Linha {
                    linha,
                    grupo: grupo.clone(),
                    texto: texto_de_busca(item),
                });
            }
            if algum {
                self.pagina.add(&grupo);
                self.grupos.borrow_mut().push(grupo);
            }
        }
        self.pilha.set_visible_child_name("lista");
    }

    fn linha(self: &Rc<Self>, item: &Disponivel, instalada: bool) -> adw::ActionRow {
        let l = adw::ActionRow::builder()
            .use_markup(false)
            .title(&item.nome)
            .build();
        if !item.descricao.is_empty() {
            l.set_subtitle(&item.descricao);
        }
        l.set_tooltip_text(Some(&format!("{} ({})", item.id, item.remoto)));
        let feita = gtk::Image::builder()
            .icon_name("object-select-symbolic")
            .tooltip_text("Instalada")
            .visible(instalada)
            .build();
        let espera = adw::Spinner::builder().visible(false).build();
        let botao = gtk::Button::builder()
            .label("Instalar")
            .valign(gtk::Align::Center)
            .visible(!instalada)
            .build();
        for w in [
            feita.upcast_ref::<gtk::Widget>(),
            espera.upcast_ref(),
            botao.upcast_ref(),
        ] {
            l.add_suffix(w);
        }
        let item = item.clone();
        let eu = Rc::downgrade(self);
        botao.connect_clicked(move |botao| {
            let Some(eu) = eu.upgrade() else { return };
            botao.set_visible(false);
            espera.set_visible(true);
            let (botao, espera, feita, item) =
                (botao.clone(), espera.clone(), feita.clone(), item.clone());
            glib::spawn_future_local(async move {
                let nome = item.nome.clone();
                let r = gio::spawn_blocking(move || loja::instalar(&item))
                    .await
                    .unwrap_or_else(|_| Err("a instalação falhou".into()));
                espera.set_visible(false);
                match r {
                    Ok(()) => {
                        feita.set_visible(true);
                        (eu.instalou)();
                    }
                    Err(e) => {
                        botao.set_visible(true);
                        eu.avisar(&format!("{nome}: {e}"));
                    }
                }
            });
        });
        l
    }

    fn filtrar(&self, busca: &str) {
        let mut visiveis = BTreeSet::new();
        for l in self.linhas.borrow().iter() {
            let mostra = combina(&l.texto, busca);
            l.linha.set_visible(mostra);
            if mostra {
                visiveis.insert(l.grupo.clone());
            }
        }
        for g in self.grupos.borrow().iter() {
            g.set_visible(visiveis.contains(g));
        }
        if self.linhas.borrow().is_empty() {
            return;
        }
        if visiveis.is_empty() {
            self.vazio.set_icon_name(Some("system-search-symbolic"));
            self.vazio.set_title("Nada encontrado");
            self.vazio.set_description(None);
            self.pilha.set_visible_child_name("vazio");
        } else {
            self.pilha.set_visible_child_name("lista");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_busca_acha_cada_palavra_em_qualquer_ordem() {
        let d = Disponivel {
            id: "dev.lukakuuhaku.Ancora.Driver.SafeSign".into(),
            familia: Familia::Driver,
            ramo: "master".into(),
            remoto: "ancora".into(),
            instalacao: loja::Instalacao::Usuario,
            nome: "SafeSign IC".into(),
            descricao: "Tokens GD Burti e outros".into(),
        };
        let t = texto_de_busca(&d);
        assert!(combina(&t, ""));
        assert!(combina(&t, "safesign"));
        assert!(combina(&t, "burti  GD"));
        assert!(combina(&t, "driver.safesign"));
        assert!(!combina(&t, "safenet"));
        assert!(!combina(&t, "gd safenet"));
    }
}
