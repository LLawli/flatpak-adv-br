//! O serviço em segundo plano: subir a partir da janela e, se a pessoa quiser,
//! com a sessão, pelo portal Background.
//!
//! O serviço é um processo à parte da janela, que segue depois de ela fechar.
//! Começa num grupo de sessão próprio, para que um sinal ao grupo da janela
//! (Ctrl+C no terminal, por exemplo) não o leve junto.

use crate::estado;
use gtk::gio::{self, prelude::*};
use gtk::glib::{self, Variant, VariantDict};
use std::fs::OpenOptions;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Quanto esperar o serviço antigo soltar a trava ao reiniciar.
const ESPERA_PARADA: Duration = Duration::from_secs(5);

/// Onde o serviço iniciado pela janela escreve o log.
pub fn log(dir: &Path) -> std::path::PathBuf {
    dir.join("servico.log")
}

/// Sobe o serviço, se ainda não houver um.
pub fn iniciar_servico() -> Result<(), String> {
    let dir = estado::diretorio();
    if estado::rodando(&dir) {
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log(&dir))
        .map_err(|e| e.to_string())?;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = Command::new(exe);
    cmd.arg("servir")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    // SAFETY: entre o fork e o exec só chamamos funções seguras para isso.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut filho = cmd.spawn().map_err(|e| format!("ancora servir: {e}"))?;
    // Recolhe o filho quando ele sair, para não deixar um zumbi enquanto a
    // janela estiver aberta.
    std::thread::spawn(move || filho.wait());
    Ok(())
}

/// Para o serviço que estiver rodando (mesmo o de outra instância do
/// sandbox, como o do início automático) e sobe um nesta, que enxerga as
/// extensões instaladas depois que o outro começou.
pub fn reiniciar_servico() -> Result<(), String> {
    parar_servico()?;
    iniciar_servico()
}

/// Pede ao serviço que pare e espera ele soltar a trava.
fn parar_servico() -> Result<(), String> {
    let dir = estado::diretorio();
    if !estado::rodando(&dir) {
        return Ok(());
    }
    estado::pedir_parada(&dir).map_err(|e| e.to_string())?;
    let limite = Instant::now() + ESPERA_PARADA;
    while estado::rodando(&dir) {
        if Instant::now() > limite {
            estado::descartar_parada(&dir);
            return Err("o serviço não parou".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// O que o host roda para abrir o Âncora de novo: espera a janela velha
/// soltar o nome dela no D-Bus da sessão (até 10 s), senão a nova só
/// repassaria a ativação à velha e sairia, e então o `flatpak run`. O
/// `flatpak ps` não serve para isso: ele segue listando instâncias mortas.
const REABRIR: &str = r#"exec </dev/null >/dev/null 2>&1
if command -v gdbus >/dev/null; then
    i=0
    while [ "$i" -lt 100 ] && gdbus call --session --dest org.freedesktop.DBus \
        --object-path /org/freedesktop/DBus --method org.freedesktop.DBus.NameHasOwner "$1" |
        grep -q true; do
        sleep 0.1; i=$((i + 1))
    done
else
    sleep 2
fi
exec flatpak run "$1""#;

/// Prepara a volta do Âncora num sandbox novo, que monta as extensões como
/// estão agora no host. Para o serviço (o novo sobe com a janela nova) e deixa
/// no host um processo esperando esta janela sair; quem chama sai logo depois.
///
/// O `setsid -f` volta na hora, e o `flatpak-spawn` com ele. Nenhuma saída do
/// processo do host pode vir para cá: ler um pipe que ele herdou esperaria o
/// fim dele, que espera o fim desta janela.
pub fn reabrir() -> Result<(), String> {
    if !Path::new("/.flatpak-info").is_file() {
        return Err("fora do Flatpak não há o que reabrir".into());
    }
    parar_servico()?;
    let status = Command::new("flatpak-spawn")
        .args(["--host", "setsid", "-f", "sh", "-c", REABRIR, "sh"])
        .arg(crate::APP_ID)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("flatpak-spawn: {e}"))?;
    if !status.success() {
        return Err("o host não aceitou abrir o Âncora de novo".into());
    }
    Ok(())
}

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const CAMINHO_PORTAL: &str = "/org/freedesktop/portal/desktop";

/// Pede ao portal Background que o serviço comece com a sessão, ou que deixe
/// de começar. `feito` recebe o que o portal concedeu.
pub fn inicio_automatico(ligar: bool, feito: impl FnOnce(Result<bool, String>) + 'static) {
    gio::bus_get(
        gio::BusType::Session,
        gio::Cancellable::NONE,
        move |conexao| match conexao {
            Ok(c) => pedir(&c, ligar, feito),
            Err(e) => feito(Err(e.to_string())),
        },
    );
}

fn pedir(
    conexao: &gio::DBusConnection,
    ligar: bool,
    feito: impl FnOnce(Result<bool, String>) + 'static,
) {
    let Some(nome) = conexao.unique_name() else {
        feito(Err("sem nome no barramento".into()));
        return;
    };
    // A resposta chega como sinal num objeto cujo caminho sai do nome de quem
    // pede e de um token escolhido aqui: assinar antes de pedir evita perder
    // uma resposta rápida.
    let token = format!("ancora{}", glib::random_int());
    let remetente = nome.trim_start_matches(':').replace('.', "_");
    let pedido = format!("{CAMINHO_PORTAL}/request/{remetente}/{token}");

    let feito = std::rc::Rc::new(std::cell::RefCell::new(Some(feito)));
    let assinatura: std::rc::Rc<std::cell::RefCell<Option<gio::SignalSubscription>>> =
        Default::default();
    {
        let feito = feito.clone();
        let guarda = assinatura.clone();
        *assinatura.borrow_mut() = Some(conexao.subscribe_to_signal(
            Some(PORTAL),
            Some("org.freedesktop.portal.Request"),
            Some("Response"),
            Some(&pedido),
            None,
            gio::DBusSignalFlags::NONE,
            move |sinal| {
                let resultado = resposta(sinal.parameters);
                if let Some(f) = feito.borrow_mut().take() {
                    f(resultado);
                }
                guarda.borrow_mut().take();
            },
        ));
    }

    let opcoes = VariantDict::new(None);
    opcoes.insert("handle_token", &token);
    opcoes.insert("reason", "Publicar os tokens assim que a sessão começar");
    opcoes.insert("autostart", ligar);
    opcoes.insert("commandline", vec!["ancora", "servir"]);
    opcoes.insert("dbus-activatable", false);
    let parametros = Variant::tuple_from_iter(["".to_variant(), opcoes.end()]);
    conexao.call(
        Some(PORTAL),
        CAMINHO_PORTAL,
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&parametros),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        move |r| {
            if let Err(e) = r {
                assinatura.borrow_mut().take();
                // Sem a implementação do portal (Hyprland, Sway), a interface
                // nem existe.
                let texto = if e.matches(gio::DBusError::UnknownMethod)
                    || e.matches(gio::DBusError::UnknownInterface)
                {
                    "esta sessão não oferece o portal Background".to_owned()
                } else {
                    e.message().to_owned()
                };
                if let Some(f) = feito.borrow_mut().take() {
                    f(Err(texto));
                }
            }
        },
    );
}

/// `Response(u código, a{sv} resultados)`: 0 é aceito, e `autostart` diz o
/// que ficou valendo.
fn resposta(parametros: &Variant) -> Result<bool, String> {
    let (codigo, resultados) = parametros
        .get::<(u32, VariantDict)>()
        .ok_or("resposta do portal num formato desconhecido")?;
    match codigo {
        0 => Ok(resultados
            .lookup::<bool>("autostart")
            .ok()
            .flatten()
            .unwrap_or(false)),
        1 => Err("recusado".into()),
        _ => Err("o portal não concluiu o pedido".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resposta_do_portal(codigo: u32, autostart: Option<bool>) -> Variant {
        let d = VariantDict::new(None);
        if let Some(a) = autostart {
            d.insert("autostart", a);
            d.insert("background", true);
        }
        Variant::tuple_from_iter([codigo.to_variant(), d.end()])
    }

    #[test]
    fn le_o_que_o_portal_concedeu() {
        assert_eq!(resposta(&resposta_do_portal(0, Some(true))), Ok(true));
        assert_eq!(resposta(&resposta_do_portal(0, Some(false))), Ok(false));
        assert_eq!(resposta(&resposta_do_portal(0, None)), Ok(false));
        assert!(resposta(&resposta_do_portal(1, None)).is_err());
        assert!(resposta(&resposta_do_portal(2, None)).is_err());
        assert!(resposta(&"x".to_variant()).is_err());
    }
}
