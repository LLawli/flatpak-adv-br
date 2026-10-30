//! A janela: os tokens e os certificados, as extensões instaladas, e os
//! aplicativos e navegadores que usam os tokens.
//!
//! A janela não publica nada sozinha: o serviço (`ancora servir`) publica os
//! drivers, e ela lê o que ele está fazendo (ver `estado`). O que demora
//! (perguntar ao host, abrir bancos NSS, medir com o sentinela) roda fora da
//! thread da interface, uma operação por vez.
//!
//! Cada controle se explica pela forma: um interruptor liga e desliga, um
//! ícone de aviso traz o motivo na dica, e só o que a pessoa precisa fazer
//! (abrir, fechar ou marcar algo) vira texto.

use crate::aplicativos::{self, Aplicativo, Banco, Receita};
use crate::certificado::{self, Certificado, Data, Prazo, Token};
use crate::estado::{self, Situacao};
use crate::extensao::{self, Encontrada, Familia, Ferramenta};
use crate::nativos::{self, Destino};
use crate::{APP_ID, NOME, assinadores, dialogo_extensoes, fundo, host, loja, verificacao};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// De quanto em quanto tempo a janela relê o estado do serviço.
const INTERVALO_SERVICO: Duration = Duration::from_secs(1);
/// Quanto a leitura dos certificados de um driver pode levar.
const LIMITE_CERTIFICADOS: Duration = Duration::from_secs(15);
/// O que a extensão do navegador chama de cada nome de biblioteca que o
/// assinador carrega (Web PKI e WebSigner).
const OPCOES_DA_EXTENSAO: [(&str, &str); 1] = [("libaetpkss.so.3", "Dispositivos SafeSign AET")];
const INICIO_AUTOMATICO: &str = "inicio-automatico";

// --- o que mostrar ----------------------------------------------------------

/// Como um driver está no serviço.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Publicacao {
    /// O serviço não está rodando.
    SemServico,
    /// O serviço começou antes de a extensão ser instalada.
    Ausente,
    Iniciando,
    Publicado,
    ComProblema(String),
    Desligado,
}

/// Um driver pode ter vários módulos: basta um com problema para avisar.
fn publicacao(servico: Option<&estado::Estado>, extensao: &str) -> Publicacao {
    let Some(servico) = servico else {
        return Publicacao::SemServico;
    };
    let modulos: Vec<_> = servico
        .drivers
        .iter()
        .filter(|d| d.extensao == extensao)
        .collect();
    if modulos.is_empty() {
        return Publicacao::Ausente;
    }
    let problemas: Vec<String> = modulos
        .iter()
        .filter_map(|d| match &d.situacao {
            Situacao::ComErro { motivo } | Situacao::Parado { motivo } => {
                Some(if modulos.len() > 1 {
                    format!("{}: {motivo}", d.rotulo)
                } else {
                    motivo.clone()
                })
            }
            _ => None,
        })
        .collect();
    if !problemas.is_empty() {
        return Publicacao::ComProblema(problemas.join("\n"));
    }
    if modulos.iter().all(|d| d.situacao == Situacao::Desligado) {
        return Publicacao::Desligado;
    }
    if modulos.iter().any(|d| d.situacao == Situacao::Iniciando) {
        return Publicacao::Iniciando;
    }
    Publicacao::Publicado
}

/// O que a pessoa precisa fazer para o aplicativo ficar pronto, quando é
/// algo que só ela pode fazer, ou o aviso de que ele não foi medido.
fn legenda_aplicativo(app: &Aplicativo, s: &aplicativos::Situacao) -> Option<String> {
    match &s.banco {
        Banco::EmUso => Some(format!("Feche o {} para concluir", app.nome)),
        Banco::Falta(o_que) if o_que.starts_with("abra ") => Some(maiuscula(o_que)),
        Banco::NaoVerificado if s.permissao => Some("Não verificado".into()),
        _ => None,
    }
}

fn legenda_nativo(destino: Destino, s: &nativos::Situacao) -> Option<String> {
    if s.em_uso {
        Some(format!("Feche o {} para mudar", destino.nome()))
    } else if s.publicado() && s.verificados < s.bancos {
        Some("Não verificado".into())
    } else {
        None
    }
}

/// O título, a legenda e o destaque da linha de um certificado.
fn linha_certificado(c: &Certificado, hoje: Data) -> (String, String, Option<&'static str>) {
    let (nome, documento) = certificado::titular(&c.titular);
    let (validade, classe) = match c.validade.prazo(hoje) {
        Prazo::Valido => (format!("até {}", c.validade), None),
        Prazo::Vencendo(0) => ("vence hoje".to_owned(), Some("warning")),
        Prazo::Vencendo(1) => ("vence amanhã".to_owned(), Some("warning")),
        Prazo::Vencendo(n) => (format!("vence em {n} dias"), Some("warning")),
        Prazo::Vencido => (format!("venceu em {}", c.validade), Some("error")),
    };
    let legenda = [documento, Some(c.emissor.clone()), Some(validade)]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (nome.to_owned(), legenda, classe)
}

/// A opção que a pessoa marca na extensão do navegador para o assinador
/// carregar o Âncora.
fn opcao_da_extensao(bibliotecas: &[String]) -> Option<&'static str> {
    bibliotecas.iter().find_map(|b| {
        OPCOES_DA_EXTENSAO
            .iter()
            .find(|(nome, _)| nome == b)
            .map(|(_, opcao)| *opcao)
    })
}

fn maiuscula(texto: &str) -> String {
    let mut c = texto.chars();
    match c.next() {
        Some(p) => p.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

// --- o que é lido -----------------------------------------------------------

/// O que custa ler (pergunta ao host, abre bancos NSS): lido ao abrir a
/// janela, quando ela volta ao foco e depois de cada operação.
#[derive(Debug, Clone, PartialEq)]
struct Retrato {
    extensoes: Vec<Encontrada>,
    desligados: BTreeSet<String>,
    assinadores: BTreeSet<String>,
    aplicativos: Result<Vec<(Aplicativo, aplicativos::Situacao)>, String>,
    nativos: Vec<(Destino, nativos::Situacao)>,
    /// As extensões instaladas no host, que podem não ser as montadas aqui.
    instaladas: Result<Vec<loja::Instalada>, String>,
    /// O host mudou desde que este sandbox começou: só um Âncora aberto de
    /// novo vê as extensões como elas estão.
    reinicio_pendente: bool,
}

fn retratar() -> Retrato {
    let home = host::home();
    let dados = estado::dados();
    let aplicativos = aplicativos::instalados().map(|instalados| {
        aplicativos::CONHECIDOS
            .iter()
            .filter(|a| instalados.contains(a.id))
            .map(|a| (*a, aplicativos::situacao(a, &home, &dados)))
            .collect()
    });
    let nativos = Destino::TODOS
        .into_iter()
        .map(|d| (d, nativos::situacao(d, &home, &dados)))
        .filter(|(_, s)| s.bancos > 0)
        .collect();
    let instaladas = loja::instaladas();
    let reinicio_pendente = match (&instaladas, std::fs::read_to_string("/.flatpak-info")) {
        (Ok(i), Ok(info)) => loja::reinicio_pendente(&loja::montadas(&info), i),
        _ => false,
    };
    Retrato {
        extensoes: extensao::montadas().to_vec(),
        desligados: estado::desligados(&dados),
        assinadores: assinadores::ligados(&dados),
        aplicativos,
        nativos,
        instaladas,
        reinicio_pendente,
    }
}

/// Os certificados de um driver, e para que lista de tokens foram lidos.
struct Leitura {
    tokens: Vec<String>,
    /// `None` enquanto lê.
    resultado: Option<Result<Vec<Token>, String>>,
}

// --- a janela ---------------------------------------------------------------

/// Um grupo cujas linhas são refeitas a cada mudança.
struct Secao {
    grupo: adw::PreferencesGroup,
    linhas: RefCell<Vec<gtk::Widget>>,
}

impl Secao {
    fn new(titulo: &str) -> Secao {
        Secao {
            grupo: adw::PreferencesGroup::builder().title(titulo).build(),
            linhas: RefCell::default(),
        }
    }

    fn limpar(&self) {
        for w in self.linhas.borrow_mut().drain(..) {
            self.grupo.remove(&w);
        }
    }

    fn por(&self, w: &impl IsA<gtk::Widget>) {
        self.grupo.add(w);
        self.linhas.borrow_mut().push(w.clone().upcast());
    }
}

struct Janela {
    janela: adw::ApplicationWindow,
    avisos: adw::ToastOverlay,
    tokens: Secao,
    extensoes: Secao,
    aplicativos: Secao,
    nativos: Secao,
    /// O último estado lido do serviço; `None` dentro quer dizer parado.
    servico: RefCell<Option<Option<estado::Estado>>>,
    retrato: RefCell<Option<Retrato>>,
    certificados: RefCell<BTreeMap<PathBuf, Leitura>>,
    retratando: Cell<bool>,
    /// Uma operação demorada em andamento.
    ocupado: Cell<bool>,
    /// Os interruptores que disparam operações demoradas, para travá-los
    /// enquanto uma roda.
    interruptores: RefCell<Vec<gtk::Switch>>,
    reinicio: adw::Banner,
    /// As extensões instaladas com versão nova no remote.
    atualizacoes: RefCell<BTreeSet<String>>,
    /// As extensões sendo atualizadas ou removidas agora.
    no_flatpak: RefCell<BTreeSet<String>>,
    /// A pergunta de remover aberta, para não abrir outra por cima.
    confirmacao: RefCell<Option<adw::AlertDialog>>,
}

fn linha(titulo: &str, legenda: Option<&str>, icone: &str) -> adw::ActionRow {
    let l = adw::ActionRow::builder()
        .use_markup(false)
        .title(titulo)
        .build();
    if let Some(s) = legenda {
        l.set_subtitle(s);
    }
    if !icone.is_empty() {
        l.add_prefix(&gtk::Image::from_icon_name(icone));
    }
    l
}

fn linha_vazia(titulo: &str, icone: &str) -> adw::ActionRow {
    let l = linha(titulo, None, icone);
    l.add_css_class("dim-label");
    l
}

fn interruptor(ligado: bool) -> gtk::Switch {
    gtk::Switch::builder()
        .active(ligado)
        .state(ligado)
        .valign(gtk::Align::Center)
        .build()
}

/// O indicador de trabalho de uma linha com operação demorada, escondido até
/// ela começar, e à esquerda do interruptor que vem depois.
fn espera_em(l: &adw::ActionRow) -> adw::Spinner {
    let espera = adw::Spinner::builder().visible(false).build();
    l.add_suffix(&espera);
    espera
}

fn icone_da_familia(familia: Familia) -> &'static str {
    match familia {
        Familia::Driver => "application-x-addon-symbolic",
        Familia::Assinador => "document-edit-symbolic",
        Familia::App => "applications-system-symbolic",
    }
}

fn icone_do_aplicativo(app: &Aplicativo) -> &'static str {
    match app.receita {
        Receita::Firefox | Receita::Chromium => "web-browser-symbolic",
        Receita::Poppler => "x-office-document-symbolic",
        Receita::Pronto => "document-edit-symbolic",
    }
}

impl Janela {
    fn avisar(&self, texto: &str) {
        if !texto.is_empty() {
            self.avisos.add_toast(adw::Toast::new(texto));
        }
    }

    fn servico_atual(&self) -> Option<estado::Estado> {
        self.servico.borrow().clone().flatten()
    }

    /// Relê o estado do serviço e refaz o que depende dele.
    fn atualizar_servico(self: &Rc<Self>) {
        let novo = estado::ler(&estado::diretorio());
        if self.servico.borrow().as_ref() == Some(&novo) {
            return;
        }
        *self.servico.borrow_mut() = Some(novo);
        self.ler_certificados();
        self.mostrar_tokens();
        // Refazer as extensões no meio de uma operação levaria junto a linha
        // que mostra que ela está rodando; o fim dela refaz tudo.
        if !self.ocupado.get() {
            self.mostrar_extensoes();
        }
    }

    /// Guarda um interruptor que dispara operação demorada, esquecendo os
    /// que já saíram da janela.
    fn registrar(&self, sw: &gtk::Switch) {
        let mut lista = self.interruptores.borrow_mut();
        lista.retain(|s| s.root().is_some());
        lista.push(sw.clone());
    }

    /// Relê, fora da thread da interface, o que custa ler.
    fn atualizar_tudo(self: &Rc<Self>) {
        if self.ocupado.get() || self.retratando.replace(true) {
            return;
        }
        let eu = self.clone();
        glib::spawn_future_local(async move {
            let retrato = gio::spawn_blocking(retratar).await;
            eu.retratando.set(false);
            let Ok(retrato) = retrato else { return };
            if eu.ocupado.get() || eu.retrato.borrow().as_ref() == Some(&retrato) {
                return;
            }
            *eu.retrato.borrow_mut() = Some(retrato);
            eu.mostrar_tokens();
            eu.mostrar_extensoes();
            eu.mostrar_aplicativos();
            eu.mostrar_nativos();
        });
    }

    /// Lê os certificados dos drivers cujos tokens mudaram.
    fn ler_certificados(self: &Rc<Self>) {
        let publicados: BTreeMap<PathBuf, Vec<String>> = self
            .servico_atual()
            .map(|s| s.drivers)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|d| match d.situacao {
                Situacao::Publicado { tokens } if !tokens.is_empty() => Some((d.socket, tokens)),
                _ => None,
            })
            .collect();
        let mut cache = self.certificados.borrow_mut();
        cache.retain(|socket, _| publicados.contains_key(socket));
        for (socket, tokens) in publicados {
            if cache.get(&socket).is_some_and(|l| l.tokens == tokens) {
                continue;
            }
            cache.insert(
                socket.clone(),
                Leitura {
                    tokens: tokens.clone(),
                    resultado: None,
                },
            );
            let eu = self.clone();
            glib::spawn_future_local(async move {
                let s = socket.clone();
                let resultado = gio::spawn_blocking(move || {
                    certificado::listar_com_limite(&s, LIMITE_CERTIFICADOS)
                })
                .await
                .unwrap_or_else(|_| Err("a leitura falhou".into()));
                let mut cache = eu.certificados.borrow_mut();
                match cache.get_mut(&socket) {
                    // Os tokens mudaram de novo durante a leitura: outra já
                    // foi pedida.
                    Some(l) if l.tokens == tokens => l.resultado = Some(resultado),
                    _ => return,
                }
                drop(cache);
                eu.mostrar_tokens();
            });
        }
    }

    fn nome_da_extensao(&self, id: &str) -> String {
        self.retrato
            .borrow()
            .as_ref()
            .and_then(|r| r.extensoes.iter().find(|e| e.id == id))
            .and_then(|e| e.descritor.as_ref().ok())
            .map(|d| d.nome.clone())
            .unwrap_or_else(|| id.to_owned())
    }

    fn mostrar_tokens(self: &Rc<Self>) {
        self.tokens.limpar();
        let servico = self.servico.borrow().clone();
        let Some(Some(servico)) = servico else {
            if servico.is_some() {
                self.tokens.por(&self.linha_servico_parado());
            }
            return;
        };
        let hoje = Data::hoje();
        let cache = self.certificados.borrow();
        let mut algum = false;
        for d in &servico.drivers {
            let Situacao::Publicado { tokens } = &d.situacao else {
                continue;
            };
            let leitura = cache.get(&d.socket);
            for rotulo in tokens {
                algum = true;
                let expansor = adw::ExpanderRow::builder()
                    .use_markup(false)
                    .title(rotulo)
                    .subtitle(self.nome_da_extensao(&d.extensao))
                    .build();
                expansor.add_prefix(&gtk::Image::from_icon_name("auth-smartcard-symbolic"));
                match leitura.and_then(|l| l.resultado.as_ref()) {
                    None => expansor.add_suffix(&adw::Spinner::new()),
                    Some(Err(e)) => {
                        let aviso = gtk::Image::from_icon_name("dialog-warning-symbolic");
                        aviso.set_tooltip_text(Some(e));
                        expansor.add_suffix(&aviso);
                    }
                    Some(Ok(lidos)) => {
                        let certificados = lidos
                            .iter()
                            .filter(|t| &t.rotulo == rotulo)
                            .flat_map(|t| &t.certificados);
                        let mut nenhum = true;
                        for c in certificados {
                            nenhum = false;
                            let (titulo, legenda, classe) = linha_certificado(c, hoje);
                            let l =
                                linha(&titulo, Some(&legenda), "application-certificate-symbolic");
                            if let Some(classe) = classe {
                                l.add_css_class(classe);
                            }
                            expansor.add_row(&l);
                        }
                        if nenhum {
                            expansor.add_row(&linha_vazia("Nenhum certificado visível", ""));
                        }
                        expansor.set_expanded(true);
                    }
                }
                self.tokens.por(&expansor);
            }
        }
        if !algum {
            self.tokens.por(&linha_vazia(
                "Nenhum token conectado",
                "auth-smartcard-symbolic",
            ));
        }
    }

    fn linha_servico_parado(self: &Rc<Self>) -> adw::ActionRow {
        let l = linha("Serviço parado", None, "dialog-warning-symbolic");
        l.add_css_class("warning");
        let botao = gtk::Button::builder()
            .label("Iniciar")
            .valign(gtk::Align::Center)
            .css_classes(["suggested-action"])
            .build();
        let eu = Rc::downgrade(self);
        botao.connect_clicked(move |_| {
            if let Some(eu) = eu.upgrade()
                && let Err(e) = fundo::iniciar_servico()
            {
                eu.avisar(&e);
            }
        });
        l.add_suffix(&botao);
        l
    }

    fn mostrar_extensoes(self: &Rc<Self>) {
        self.extensoes.limpar();
        let retrato = self.retrato.borrow().clone();
        let Some(retrato) = retrato else { return };
        self.reinicio.set_revealed(retrato.reinicio_pendente);
        if retrato.extensoes.is_empty() {
            self.extensoes.por(&linha_vazia(
                "Nenhuma extensão instalada",
                "application-x-addon-symbolic",
            ));
            return;
        }
        let servico = self.servico.borrow().clone().flatten();
        for e in &retrato.extensoes {
            let d = match &e.descritor {
                Ok(d) => d,
                Err(erro) => {
                    let l = linha(&e.id, Some(&erro.to_string()), "dialog-warning-symbolic");
                    l.add_css_class("warning");
                    self.sufixos_loja(&l, e, &retrato);
                    self.extensoes.por(&l);
                    continue;
                }
            };
            let fabricante = (!d.fabricante.is_empty()).then_some(d.fabricante.as_str());
            let l = linha(&d.nome, fabricante, "");
            match d.icone.as_deref().filter(|i| i.is_file()) {
                Some(i) => {
                    let imagem = gtk::Image::from_file(i);
                    imagem.set_pixel_size(32);
                    l.add_prefix(&imagem);
                }
                None => l.add_prefix(&gtk::Image::from_icon_name(icone_da_familia(e.familia))),
            }
            if !d.descricao.is_empty() {
                l.set_tooltip_text(Some(&d.descricao));
            }
            for f in &d.ferramentas {
                l.add_suffix(&self.botao_ferramenta(f));
            }
            match e.familia {
                Familia::Driver => {
                    self.sufixos_driver(&l, &e.id, servico.as_ref(), &retrato.desligados)
                }
                Familia::Assinador => {
                    let ligado = retrato.assinadores.contains(&e.id);
                    let opcao = d
                        .assinadores
                        .iter()
                        .find_map(|a| opcao_da_extensao(&a.pkcs11));
                    if ligado && let Some(opcao) = opcao {
                        l.set_subtitle(&format!("Marque “{opcao}” na extensão do navegador"));
                    }
                    self.interruptor_assinador(&l, &e.id, ligado);
                }
                Familia::App => {}
            }
            self.sufixos_loja(&l, e, &retrato);
            self.extensoes.por(&l);
        }
    }

    /// Atualizar e remover, pelo host. Uma extensão que saiu do host segue
    /// montada até o Âncora abrir de novo, e fica esmaecida até lá.
    fn sufixos_loja(self: &Rc<Self>, l: &adw::ActionRow, e: &Encontrada, retrato: &Retrato) {
        let id = format!("{APP_ID}.{}.{}", e.familia.ponto(), e.id);
        let Ok(instaladas) = &retrato.instaladas else {
            return;
        };
        let Some(instalada) = instaladas.iter().find(|i| i.id == id) else {
            l.set_sensitive(false);
            return;
        };
        if self.no_flatpak.borrow().contains(&id) {
            l.add_suffix(&adw::Spinner::new());
            return;
        }
        if self.atualizacoes.borrow().contains(&id) {
            let botao = gtk::Button::builder()
                .icon_name("software-update-available-symbolic")
                .tooltip_text("Atualizar")
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .build();
            let eu = Rc::downgrade(self);
            let i = instalada.clone();
            botao.connect_clicked(move |_| {
                if let Some(eu) = eu.upgrade() {
                    let i = i.clone();
                    eu.no_flatpak_fazer(&i.id.clone(), move || loja::atualizar(&i));
                }
            });
            l.add_suffix(&botao);
        }
        let menu = gio::Menu::new();
        menu.append(
            Some("Remover"),
            Some(&format!("win.remover::{}", instalada.id)),
        );
        l.add_suffix(
            &gtk::MenuButton::builder()
                .icon_name("view-more-symbolic")
                .menu_model(&menu)
                .valign(gtk::Align::Center)
                .css_classes(["flat"])
                .tooltip_text("Mais")
                .build(),
        );
    }

    /// Pergunta antes de remover: tirar um driver tira os tokens dele de
    /// todos os aplicativos.
    fn confirmar_remocao(self: &Rc<Self>, id: &str) {
        if self.confirmacao.borrow().is_some() {
            return;
        }
        let Some(instalada) = self
            .retrato
            .borrow()
            .as_ref()
            .and_then(|r| r.instaladas.as_ref().ok())
            .and_then(|l| l.iter().find(|i| i.id == id).cloned())
        else {
            return;
        };
        let nome = loja::familia_de(id)
            .map(|(_, curto)| self.nome_da_extensao(curto))
            .unwrap_or_else(|| id.to_owned());
        let alerta = adw::AlertDialog::new(Some(&format!("Remover {nome}?")), None);
        alerta.add_responses(&[("cancelar", "Cancelar"), ("remover", "Remover")]);
        alerta.set_response_appearance("remover", adw::ResponseAppearance::Destructive);
        alerta.set_default_response(Some("cancelar"));
        alerta.set_close_response("cancelar");
        let eu = Rc::downgrade(self);
        alerta.connect_response(Some("remover"), move |_, _| {
            if let Some(eu) = eu.upgrade() {
                let i = instalada.clone();
                eu.no_flatpak_fazer(&i.id.clone(), move || loja::remover(&i));
            }
        });
        {
            let eu = Rc::downgrade(self);
            alerta.connect_closed(move |_| {
                if let Some(eu) = eu.upgrade() {
                    eu.confirmacao.borrow_mut().take();
                }
            });
        }
        *self.confirmacao.borrow_mut() = Some(alerta.clone());
        alerta.present(Some(&self.janela));
    }

    /// Atualiza ou remove fora da thread da interface, com a linha mostrando
    /// que está trabalhando.
    fn no_flatpak_fazer(
        self: &Rc<Self>,
        id: &str,
        tarefa: impl FnOnce() -> Result<(), String> + Send + 'static,
    ) {
        if !self.no_flatpak.borrow_mut().insert(id.to_owned()) {
            return;
        }
        self.mostrar_extensoes();
        let eu = self.clone();
        let id = id.to_owned();
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(tarefa)
                .await
                .unwrap_or_else(|_| Err("a operação falhou".into()));
            eu.no_flatpak.borrow_mut().remove(&id);
            match r {
                Ok(()) => {
                    eu.atualizacoes.borrow_mut().remove(&id);
                }
                Err(e) => eu.avisar(&e),
            }
            *eu.retrato.borrow_mut() = None;
            eu.atualizar_tudo();
        });
    }

    /// Pergunta aos remotes o que tem versão nova. Usa a rede: só ao abrir.
    fn buscar_atualizacoes(self: &Rc<Self>) {
        let eu = self.clone();
        glib::spawn_future_local(async move {
            let r =
                gio::spawn_blocking(|| loja::atualizacoes(&loja::instaladas().unwrap_or_default()))
                    .await;
            if let Ok(ids) = r
                && *eu.atualizacoes.borrow() != ids
            {
                *eu.atualizacoes.borrow_mut() = ids;
                eu.mostrar_extensoes();
            }
        });
    }

    fn reabrir(self: &Rc<Self>) {
        self.reinicio.set_sensitive(false);
        let eu = self.clone();
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(fundo::reabrir)
                .await
                .unwrap_or_else(|_| Err("a operação falhou".into()));
            match r {
                Ok(()) => {
                    if let Some(app) = eu.janela.application() {
                        app.quit();
                    }
                }
                Err(e) => {
                    eu.reinicio.set_sensitive(true);
                    eu.avisar(&e);
                }
            }
        });
    }

    fn sufixos_driver(
        self: &Rc<Self>,
        l: &adw::ActionRow,
        id: &str,
        servico: Option<&estado::Estado>,
        desligados: &BTreeSet<String>,
    ) {
        match publicacao(servico, id) {
            Publicacao::Iniciando => l.add_suffix(&adw::Spinner::new()),
            Publicacao::ComProblema(motivo) => {
                let aviso = gtk::Image::from_icon_name("dialog-warning-symbolic");
                aviso.set_tooltip_text(Some(&motivo));
                aviso.add_css_class("warning");
                l.add_suffix(&aviso);
            }
            Publicacao::Ausente if !desligados.contains(id) => {
                let botao = gtk::Button::builder()
                    .icon_name("view-refresh-symbolic")
                    .tooltip_text("Reiniciar o serviço para usar")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                let eu = Rc::downgrade(self);
                botao.connect_clicked(move |_| {
                    if let Some(eu) = eu.upgrade() {
                        eu.reiniciar_servico();
                    }
                });
                l.add_suffix(&botao);
            }
            _ => {}
        }
        let sw = interruptor(!desligados.contains(id));
        let id = id.to_owned();
        let eu = Rc::downgrade(self);
        // Desligar é só escrever a escolha; o serviço a lê e para o driver.
        sw.connect_state_set(move |_, ligar| {
            let Some(eu) = eu.upgrade() else {
                return glib::Propagation::Proceed;
            };
            if let Err(e) = estado::ligar(&estado::dados(), &id, ligar) {
                eu.avisar(&e.to_string());
                return glib::Propagation::Stop;
            }
            if let Some(r) = eu.retrato.borrow_mut().as_mut() {
                if ligar {
                    r.desligados.remove(&id);
                } else {
                    r.desligados.insert(id.clone());
                }
            }
            glib::Propagation::Proceed
        });
        l.add_suffix(&sw);
        l.set_activatable_widget(Some(&sw));
    }

    fn interruptor_assinador(self: &Rc<Self>, l: &adw::ActionRow, id: &str, ligado: bool) {
        let sw = interruptor(ligado);
        let id = id.to_owned();
        let eu = Rc::downgrade(self);
        let espera = espera_em(l);
        sw.connect_state_set(move |sw, ligar| {
            let Some(eu) = eu.upgrade() else {
                return glib::Propagation::Proceed;
            };
            let mut escolhidos = assinadores::ligados(&estado::dados());
            if ligar {
                escolhidos.insert(id.clone());
            } else {
                escolhidos.remove(&id);
            }
            eu.operar(sw, &espera, move || {
                let dados = estado::dados();
                assinadores::ligar(&dados, &escolhidos)?;
                let plano = assinadores::sincronizar(&host::home(), &dados)?;
                Ok(aviso_assinadores(&plano, ligar))
            })
        });
        self.registrar(&sw);
        l.add_suffix(&sw);
        l.set_activatable_widget(Some(&sw));
    }

    fn botao_ferramenta(self: &Rc<Self>, f: &Ferramenta) -> gtk::Button {
        let botao = gtk::Button::builder()
            .label(&f.nome)
            .valign(gtk::Align::Center)
            .build();
        let f = f.clone();
        let eu = Rc::downgrade(self);
        botao.connect_clicked(move |_| {
            let r = std::process::Command::new(&f.executavel)
                .args(&f.argumentos)
                .envs(&f.ambiente)
                .spawn();
            match r {
                Ok(mut filho) => {
                    std::thread::spawn(move || filho.wait());
                }
                Err(e) => {
                    if let Some(eu) = eu.upgrade() {
                        eu.avisar(&format!("{}: {e}", f.nome));
                    }
                }
            }
        });
        botao
    }

    fn mostrar_aplicativos(self: &Rc<Self>) {
        self.aplicativos.limpar();
        let retrato = self.retrato.borrow().clone();
        let Some(retrato) = retrato else { return };
        let lista = match &retrato.aplicativos {
            Ok(l) => l,
            Err(e) => {
                let l = linha(
                    "Não foi possível listar os aplicativos",
                    Some(e),
                    "dialog-warning-symbolic",
                );
                l.add_css_class("warning");
                self.aplicativos.por(&l);
                self.aplicativos.grupo.set_visible(true);
                return;
            }
        };
        self.aplicativos.grupo.set_visible(!lista.is_empty());
        for (app, s) in lista {
            let l = linha(
                app.nome,
                legenda_aplicativo(app, s).as_deref(),
                icone_do_aplicativo(app),
            );
            l.set_tooltip_text(Some(app.id));
            if app.receita == Receita::Pronto {
                l.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
                self.aplicativos.por(&l);
                continue;
            }
            let sw = interruptor(s.pronto());
            let app = *app;
            let eu = Rc::downgrade(self);
            let espera = espera_em(&l);
            sw.connect_state_set(move |sw, ligar| {
                let Some(eu) = eu.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                eu.operar(sw, &espera, move || {
                    let home = host::home();
                    let dados = estado::dados();
                    if !ligar {
                        aplicativos::desfazer(&app, &home, &dados)?;
                        return Ok(String::new());
                    }
                    aplicativos::ligar_servidor_do_sistema()
                        .map_err(|e| format!("p11-kit-server.socket: {e}"))?;
                    let medidor = verificacao::medidor(&home);
                    aplicativos::configurar(&app, &home, &dados, medidor.as_ref())?;
                    Ok(format!("Feche e abra o {} de novo", app.nome))
                })
            });
            self.registrar(&sw);
            l.add_suffix(&sw);
            l.set_activatable_widget(Some(&sw));
            self.aplicativos.por(&l);
        }
    }

    fn mostrar_nativos(self: &Rc<Self>) {
        self.nativos.limpar();
        let retrato = self.retrato.borrow().clone();
        let Some(retrato) = retrato else { return };
        self.nativos.grupo.set_visible(!retrato.nativos.is_empty());
        for (destino, s) in &retrato.nativos {
            let l = linha(
                destino.nome(),
                legenda_nativo(*destino, s).as_deref(),
                "web-browser-symbolic",
            );
            let sw = interruptor(s.publicado());
            let destino = *destino;
            let eu = Rc::downgrade(self);
            let espera = espera_em(&l);
            sw.connect_state_set(move |sw, ligar| {
                let Some(eu) = eu.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                eu.operar(sw, &espera, move || {
                    let home = host::home();
                    let dados = estado::dados();
                    if ligar {
                        let proxy = nativos::proxy_do_host()?;
                        let medidor = verificacao::medidor(&home);
                        nativos::publicar(destino, &home, &dados, &proxy, medidor.as_ref())?;
                    } else {
                        nativos::despublicar(destino, &home, &dados)?;
                    }
                    Ok(format!("Feche e abra o {} de novo", destino.nome()))
                })
            });
            self.registrar(&sw);
            l.add_suffix(&sw);
            l.set_activatable_widget(Some(&sw));
            self.nativos.por(&l);
        }
    }

    /// Roda uma operação demorada fora da thread da interface. O interruptor
    /// só muda de estado quando ela termina: até lá, a linha mostra que está
    /// trabalhando, e os outros interruptores esperam.
    fn operar(
        self: &Rc<Self>,
        sw: &gtk::Switch,
        espera: &adw::Spinner,
        tarefa: impl FnOnce() -> Result<String, String> + Send + 'static,
    ) -> glib::Propagation {
        if self.ocupado.replace(true) {
            let sw = sw.clone();
            glib::idle_add_local_once(move || sw.set_active(sw.state()));
            return glib::Propagation::Stop;
        }
        for outro in self.interruptores.borrow().iter() {
            outro.set_sensitive(false);
        }
        espera.set_visible(true);
        let eu = self.clone();
        let sw = sw.clone();
        let espera = espera.clone();
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(tarefa)
                .await
                .unwrap_or_else(|_| Err("a operação falhou".into()));
            espera.set_visible(false);
            match r {
                Ok(aviso) => eu.avisar(&aviso),
                Err(e) => eu.avisar(&e),
            }
            sw.set_active(sw.state());
            eu.ocupado.set(false);
            // Refaz tudo pelo que ficou no disco, que é a verdade.
            *eu.retrato.borrow_mut() = None;
            eu.atualizar_tudo();
        });
        glib::Propagation::Stop
    }

    fn reiniciar_servico(self: &Rc<Self>) {
        let eu = self.clone();
        glib::spawn_future_local(async move {
            let r = gio::spawn_blocking(fundo::reiniciar_servico)
                .await
                .unwrap_or_else(|_| Err("a operação falhou".into()));
            if let Err(e) = r {
                eu.avisar(&e);
            }
            eu.atualizar_servico();
        });
    }
}

fn aviso_assinadores(plano: &assinadores::Plano, ligado: bool) -> String {
    if !ligado {
        return String::new();
    }
    if plano.sem_tokens.is_empty() {
        return "Feche e abra os navegadores de novo".into();
    }
    let nomes: Vec<&str> = plano
        .sem_tokens
        .iter()
        .map(|id| aplicativos::por_id(id).map_or(id.as_str(), |a| a.nome))
        .collect();
    format!(
        "Ligue {} em Aplicativos para ver os tokens",
        nomes.join(", ")
    )
}

pub fn montar(app: &adw::Application) {
    if let Some(j) = app.active_window() {
        j.present();
        return;
    }
    let menu = gio::Menu::new();
    menu.append(Some("Iniciar com a sessão"), Some("app.inicio-automatico"));
    menu.append(Some("Reiniciar o serviço"), Some("app.reiniciar"));
    menu.append(Some("Sobre o Âncora"), Some("app.sobre"));
    let cabecalho = adw::HeaderBar::new();
    cabecalho.pack_end(
        &gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&menu)
            .primary(true)
            .tooltip_text("Menu principal")
            .build(),
    );

    let j = Rc::new(Janela {
        janela: adw::ApplicationWindow::builder()
            .application(app)
            .title(NOME)
            .default_width(640)
            .default_height(760)
            .build(),
        avisos: adw::ToastOverlay::new(),
        tokens: Secao::new("Tokens"),
        extensoes: Secao::new("Extensões"),
        aplicativos: Secao::new("Aplicativos"),
        nativos: Secao::new("Navegadores do sistema"),
        servico: RefCell::default(),
        retrato: RefCell::default(),
        certificados: RefCell::default(),
        retratando: Cell::new(false),
        ocupado: Cell::new(false),
        interruptores: RefCell::default(),
        reinicio: adw::Banner::builder()
            .title("As mudanças nas extensões valem depois de reiniciar")
            .button_label("Reiniciar")
            .build(),
        atualizacoes: RefCell::default(),
        no_flatpak: RefCell::default(),
        confirmacao: RefCell::default(),
    });
    let pagina = adw::PreferencesPage::new();
    for s in [&j.tokens, &j.extensoes, &j.aplicativos, &j.nativos] {
        pagina.add(&s.grupo);
    }
    j.aplicativos.grupo.set_visible(false);
    j.nativos.grupo.set_visible(false);

    let adicionar = gtk::Button::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Adicionar extensões")
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    {
        let eu = Rc::downgrade(&j);
        adicionar.connect_clicked(move |_| {
            let Some(eu) = eu.upgrade() else { return };
            let depois = Rc::downgrade(&eu);
            dialogo_extensoes::abrir(&eu.janela, move || {
                if let Some(eu) = depois.upgrade() {
                    *eu.retrato.borrow_mut() = None;
                    eu.atualizar_tudo();
                }
            });
        });
    }
    j.extensoes.grupo.set_header_suffix(Some(&adicionar));
    {
        let eu = Rc::downgrade(&j);
        j.reinicio.connect_button_clicked(move |_| {
            if let Some(eu) = eu.upgrade() {
                eu.reabrir();
            }
        });
    }
    let remover = gio::SimpleAction::new("remover", Some(glib::VariantTy::STRING));
    {
        let eu = Rc::downgrade(&j);
        remover.connect_activate(move |_, id| {
            if let (Some(eu), Some(id)) = (eu.upgrade(), id.and_then(|v| v.str())) {
                eu.confirmar_remocao(id);
            }
        });
    }
    j.janela.add_action(&remover);

    let vista = adw::ToolbarView::new();
    vista.add_top_bar(&cabecalho);
    vista.add_top_bar(&j.reinicio);
    vista.set_content(Some(&pagina));
    j.avisos.set_child(Some(&vista));
    j.janela.set_content(Some(&j.avisos));
    acoes(app, &j);

    if let Err(e) = fundo::iniciar_servico() {
        j.avisar(&e);
    }
    // Um navegador pode ter aparecido desde a última vez.
    if !assinadores::ligados(&estado::dados()).is_empty() {
        let eu = j.clone();
        eu.ocupado.set(true);
        glib::spawn_future_local(async move {
            let r =
                gio::spawn_blocking(|| assinadores::sincronizar(&host::home(), &estado::dados()))
                    .await;
            if let Ok(Err(e)) = r {
                eu.avisar(&e);
            }
            eu.ocupado.set(false);
            eu.atualizar_tudo();
        });
    }

    j.atualizar_servico();
    j.atualizar_tudo();
    j.buscar_atualizacoes();
    {
        let eu = Rc::downgrade(&j);
        glib::timeout_add_local(INTERVALO_SERVICO, move || match eu.upgrade() {
            Some(eu) => {
                eu.atualizar_servico();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
    }
    {
        // Ao voltar para a janela: a pessoa pode ter aberto ou fechado um
        // navegador, ou instalado um aplicativo.
        let eu = Rc::downgrade(&j);
        j.janela.connect_is_active_notify(move |w| {
            if w.is_active()
                && let Some(eu) = eu.upgrade()
            {
                eu.atualizar_tudo();
            }
        });
    }
    {
        // A janela guarda a si mesma até fechar.
        let guarda = RefCell::new(Some(j.clone()));
        j.janela.connect_close_request(move |_| {
            guarda.borrow_mut().take();
            glib::Propagation::Proceed
        });
    }
    j.janela.present();
}

fn acoes(app: &adw::Application, j: &Rc<Janela>) {
    let arquivo = estado::dados().join(INICIO_AUTOMATICO);
    let inicio =
        gio::SimpleAction::new_stateful(INICIO_AUTOMATICO, None, &arquivo.is_file().to_variant());
    {
        let eu = Rc::downgrade(j);
        inicio.connect_activate(move |acao, _| {
            let ligar = !acao.state().and_then(|v| v.get::<bool>()).unwrap_or(false);
            let acao = acao.clone();
            let arquivo = arquivo.clone();
            let eu = eu.clone();
            fundo::inicio_automatico(ligar, move |r| match r {
                Ok(concedido) => {
                    acao.set_state(&concedido.to_variant());
                    let r = if concedido {
                        std::fs::create_dir_all(estado::dados())
                            .and_then(|()| std::fs::write(&arquivo, ""))
                    } else {
                        std::fs::remove_file(&arquivo).or(Ok(()))
                    };
                    if let (Err(e), Some(eu)) = (r, eu.upgrade()) {
                        eu.avisar(&e.to_string());
                    }
                }
                Err(e) => {
                    if let Some(eu) = eu.upgrade() {
                        eu.avisar(&format!("Início com a sessão: {e}"));
                    }
                }
            });
        });
    }
    app.add_action(&inicio);

    let reiniciar = gio::SimpleAction::new("reiniciar", None);
    {
        let eu = Rc::downgrade(j);
        reiniciar.connect_activate(move |_, _| {
            if let Some(eu) = eu.upgrade() {
                eu.reiniciar_servico();
            }
        });
    }
    app.add_action(&reiniciar);

    let sobre = gio::SimpleAction::new("sobre", None);
    {
        let janela = j.janela.clone();
        sobre.connect_activate(move |_, _| {
            adw::AboutDialog::builder()
                .application_name(NOME)
                .application_icon(APP_ID)
                .developer_name("Luka Kuuhaku")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/LLawli/flatpak-adv-br")
                .issue_url("https://github.com/LLawli/flatpak-adv-br/issues")
                .license_type(gtk::License::Gpl30Only)
                .build()
                .present(Some(&janela));
        });
    }
    app.add_action(&sobre);

    let sair = gio::SimpleAction::new("sair", None);
    {
        let app = app.clone();
        sair.connect_activate(move |_, _| app.quit());
    }
    app.add_action(&sair);
    app.set_accels_for_action("app.sair", &["<Control>q"]);
    app.set_accels_for_action("window.close", &["<Control>w"]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn driver(extensao: &str, rotulo: &str, situacao: Situacao) -> estado::Driver {
        estado::Driver {
            extensao: extensao.into(),
            rotulo: rotulo.into(),
            socket: PathBuf::from("/x").join(rotulo),
            situacao,
        }
    }

    #[test]
    fn um_modulo_com_problema_basta_para_avisar() {
        let servico = estado::Estado {
            drivers: vec![
                driver(
                    "SafeSign",
                    "SafeSign",
                    Situacao::Publicado { tokens: vec![] },
                ),
                driver("Duplo", "Duplo/1", Situacao::Publicado { tokens: vec![] }),
                driver(
                    "Duplo",
                    "Duplo/2",
                    Situacao::Parado {
                        motivo: "o driver não respondeu".into(),
                    },
                ),
                driver("Novo", "Novo", Situacao::Iniciando),
                driver("Fora", "Fora", Situacao::Desligado),
            ],
        };
        let s = Some(&servico);
        assert_eq!(publicacao(None, "SafeSign"), Publicacao::SemServico);
        assert_eq!(publicacao(s, "SafeSign"), Publicacao::Publicado);
        assert_eq!(
            publicacao(s, "Duplo"),
            Publicacao::ComProblema("Duplo/2: o driver não respondeu".into())
        );
        assert_eq!(publicacao(s, "Novo"), Publicacao::Iniciando);
        assert_eq!(publicacao(s, "Fora"), Publicacao::Desligado);
        assert_eq!(publicacao(s, "Instalado"), Publicacao::Ausente);
    }

    #[test]
    fn so_o_que_a_pessoa_precisa_fazer_vira_texto() {
        let firefox = aplicativos::por_id("org.mozilla.firefox").unwrap();
        let s = |permissao, banco| aplicativos::Situacao { permissao, banco };
        assert_eq!(legenda_aplicativo(&firefox, &s(true, Banco::Pronto)), None);
        assert_eq!(
            legenda_aplicativo(&firefox, &s(false, Banco::Falta("verificar".into()))),
            None
        );
        assert_eq!(
            legenda_aplicativo(&firefox, &s(true, Banco::EmUso)).as_deref(),
            Some("Feche o Firefox para concluir")
        );
        assert_eq!(
            legenda_aplicativo(
                &firefox,
                &s(
                    false,
                    Banco::Falta("abra o Firefox uma vez para criar o perfil".into())
                )
            )
            .as_deref(),
            Some("Abra o Firefox uma vez para criar o perfil")
        );
        assert_eq!(
            legenda_aplicativo(&firefox, &s(true, Banco::NaoVerificado)).as_deref(),
            Some("Não verificado")
        );
    }

    #[test]
    fn navegador_nativo_aberto_ou_sem_medida() {
        let s = |bancos, publicados, verificados, em_uso| nativos::Situacao {
            bancos,
            publicados,
            verificados,
            em_uso,
        };
        assert_eq!(legenda_nativo(Destino::Firefox, &s(2, 2, 2, false)), None);
        assert_eq!(legenda_nativo(Destino::Firefox, &s(2, 0, 0, false)), None);
        assert_eq!(
            legenda_nativo(Destino::Firefox, &s(2, 2, 1, false)).as_deref(),
            Some("Não verificado")
        );
        assert_eq!(
            legenda_nativo(Destino::Firefox, &s(1, 0, 0, true)).as_deref(),
            Some("Feche o Firefox para mudar")
        );
    }

    #[test]
    fn a_linha_do_certificado_destaca_o_vencimento() {
        let hoje = Data {
            ano: 2026,
            mes: 10,
            dia: 30,
        };
        let c = |ano, mes, dia| Certificado {
            titular: "JOSÉ DA SILVA:12345678900".into(),
            emissor: "AC TESTE v5".into(),
            validade: Data { ano, mes, dia },
        };
        assert_eq!(
            linha_certificado(&c(2027, 10, 29), hoje),
            (
                "JOSÉ DA SILVA".into(),
                "CPF 123.456.789-00 · AC TESTE v5 · até 29/10/2027".into(),
                None
            )
        );
        assert_eq!(
            linha_certificado(&c(2026, 11, 9), hoje).1,
            "CPF 123.456.789-00 · AC TESTE v5 · vence em 10 dias"
        );
        assert_eq!(linha_certificado(&c(2026, 10, 31), hoje).2, Some("warning"));
        let vencido = linha_certificado(&c(2026, 1, 2), hoje);
        assert!(vencido.1.ends_with("venceu em 02/01/2026"));
        assert_eq!(vencido.2, Some("error"));
    }

    #[test]
    fn a_opcao_da_extensao_sai_do_nome_da_biblioteca() {
        assert_eq!(
            opcao_da_extensao(&["libaetpkss.so.3".into()]),
            Some("Dispositivos SafeSign AET")
        );
        assert_eq!(opcao_da_extensao(&["libdesconhecida.so".into()]), None);
    }

    #[test]
    fn o_aviso_dos_assinadores_aponta_quem_ficou_sem_tokens() {
        let mut plano = assinadores::Plano::default();
        assert_eq!(aviso_assinadores(&plano, false), "");
        assert_eq!(
            aviso_assinadores(&plano, true),
            "Feche e abra os navegadores de novo"
        );
        plano.sem_tokens.insert("org.mozilla.firefox".into());
        assert_eq!(
            aviso_assinadores(&plano, true),
            "Ligue Firefox em Aplicativos para ver os tokens"
        );
    }
}
