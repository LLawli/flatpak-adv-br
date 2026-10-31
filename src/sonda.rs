//! `ancora sondar <socket>`: lista os tokens de um servidor pelo socket dele,
//! do mesmo jeito que um consumidor faria, sem pedir PIN.
//!
//! Roda num processo à parte porque um driver travado trava quem o chama: o
//! serviço mata a sonda que passar do limite, e segue.
//!
//! `ancora vigiar <módulo>` é a sonda que o serviço usa: carrega o driver uma
//! vez e responde a cada pedido. Carregar o driver é o que mexe no cartão: o
//! SafeSign liga o cartão e manda dezenas de comandos a cada `C_Initialize`, e
//! o desliga no `C_Finalize`. Medido com o SafeSign 4.5 e um cartão que ele
//! reconhece (o cartão virtual do gdburti-rs na leitora virtual do
//! vsmartcard), em 2026-10-31: sondando por conexões novas a cada 10 s, 6
//! ciclos de energia e 474 comandos por minuto; com o driver carregado uma vez,
//! nenhum ciclo e nenhum comando depois da carga, e o token que sai e volta
//! aparece na consulta seguinte. Com o GD Burti de verdade, o primeiro jeito
//! acompanhou o pcscd perder a comunicação USB com o token.

use cryptoki::context::{CInitializeArgs, CInitializeFlags, Pkcs11};
use cryptoki::slot::Slot;
use cryptoki_sys::{
    CK_C_INITIALIZE_ARGS, CK_FUNCTION_LIST, CK_RV, CK_SLOT_ID, CK_TOKEN_INFO, CK_TRUE, CK_ULONG,
    CKF_OS_LOCKING_OK, CKF_TOKEN_INITIALIZED, CKR_BUFFER_TOO_SMALL, CKR_OK,
};
use std::path::{Path, PathBuf};

/// Quantas vezes pedir a lista de novo se um token chegar entre a contagem e
/// a lista.
const TENTATIVAS: usize = 3;

/// O cliente do p11-kit do runtime, que fala com um `p11-kit server`.
pub fn cliente() -> PathBuf {
    PathBuf::from(format!(
        "/usr/lib/{}-linux-gnu/pkcs11/p11-kit-client.so",
        std::env::consts::ARCH
    ))
}

/// `sondar` num subprocesso, desistindo depois de `limite`.
pub fn sondar_com_limite(
    socket: &Path,
    limite: std::time::Duration,
) -> Result<Vec<String>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("sondar").arg(socket);
    let saida = crate::processo::executar(cmd, b"", limite)?;
    if saida.status.success() {
        Ok(String::from_utf8_lossy(&saida.stdout)
            .lines()
            .map(str::to_owned)
            .collect())
    } else {
        Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned())
    }
}

/// O cliente do p11-kit ligado ao servidor de `socket`, e os slots com token
/// dele. Só para o começo de um processo próprio e de vida curta, antes de
/// qualquer outra thread existir: muda o ambiente do processo.
pub fn abrir(socket: &Path) -> Result<(Pkcs11, Vec<Slot>), String> {
    // SAFETY: ver acima; quem chama roda antes de criar threads.
    unsafe {
        std::env::set_var(
            "P11_KIT_SERVER_ADDRESS",
            format!("unix:path={}", socket.display()),
        )
    };
    let cliente = cliente();
    let slots = slots_com_token(&cliente)?;
    let p11 = Pkcs11::new(&cliente).map_err(|e| format!("{}: {e}", cliente.display()))?;
    p11.initialize(CInitializeArgs::new(CInitializeFlags::OS_LOCKING_OK))
        .map_err(|e| format!("C_Initialize: {e}"))?;
    Ok((p11, slots))
}

/// Os slots com token, numa conexão própria, fechada antes de o cryptoki
/// abrir a dele: o `p11-kit-client.so` dá uma tabela nova (e uma conexão nova)
/// a cada `C_GetFunctionList`, e duas conexões juntas confundiriam a
/// supervisão, que conta as conexões para separar ocupado de travado.
///
/// Não usa o `get_slots_with_token` do cryptoki: com zero slots (o driver de
/// uma leitora sem cartão), ele pede a lista de novo com um buffer de tamanho
/// zero, o RPC do p11-kit responde `CKR_BUFFER_TOO_SMALL`, e ele repete para
/// sempre, com a mesma conta zero. Aqui a conta zero encerra a lista.
fn slots_com_token(cliente: &Path) -> Result<Vec<Slot>, String> {
    // SAFETY: carregar o cliente do p11-kit do runtime não roda nada além do
    // construtor dele.
    let biblioteca = unsafe { libloading::Library::new(cliente) }
        .map_err(|e| format!("{}: {e}", cliente.display()))?;
    let pegar: libloading::Symbol<unsafe extern "C" fn(*mut *mut CK_FUNCTION_LIST) -> CK_RV> =
        unsafe { biblioteca.get(b"C_GetFunctionList") }.map_err(|e| e.to_string())?;
    let mut tabela: *mut CK_FUNCTION_LIST = std::ptr::null_mut();
    // SAFETY: C_GetFunctionList só escreve o ponteiro da tabela.
    let rv = unsafe { pegar(&mut tabela) };
    if rv != CKR_OK || tabela.is_null() {
        return Err(format!("C_GetFunctionList: 0x{rv:x}"));
    }
    // SAFETY: a tabela vive enquanto o módulo estiver carregado, e
    // `biblioteca` o mantém até o fim da função.
    let (iniciar, finalizar, listar) = unsafe {
        let t = &*tabela;
        (t.C_Initialize, t.C_Finalize, t.C_GetSlotList)
    };
    let (Some(iniciar), Some(finalizar), Some(listar)) = (iniciar, finalizar, listar) else {
        return Err("o cliente do p11-kit não tem as funções da lista".into());
    };
    // SAFETY: sem argumentos, o módulo não usa threads de quem chama.
    let rv = unsafe { iniciar(std::ptr::null_mut()) };
    if rv != CKR_OK {
        return Err(format!("C_Initialize: 0x{rv:x}"));
    }
    let lista = listar_slots(|ids, n| {
        // SAFETY: `ids` é nulo ou tem espaço para `*n` ids.
        unsafe { listar(CK_TRUE, ids, n) }
    });
    // SAFETY: fecha a conexão aberta acima.
    unsafe { finalizar(std::ptr::null_mut()) };
    lista
}

/// O protocolo de duas chamadas do `C_GetSlotList`: a conta, depois a lista,
/// de novo se um token chegar no meio. Conta zero é lista vazia.
fn listar_slots(
    mut listar: impl FnMut(*mut CK_SLOT_ID, &mut CK_ULONG) -> CK_RV,
) -> Result<Vec<Slot>, String> {
    for _ in 0..TENTATIVAS {
        let mut n: CK_ULONG = 0;
        let rv = listar(std::ptr::null_mut(), &mut n);
        if rv != CKR_OK {
            return Err(format!("C_GetSlotList: 0x{rv:x}"));
        }
        if n == 0 {
            return Ok(vec![]);
        }
        let mut ids: Vec<CK_SLOT_ID> = vec![0; n as usize];
        let rv = listar(ids.as_mut_ptr(), &mut n);
        if rv == CKR_BUFFER_TOO_SMALL {
            continue;
        }
        if rv != CKR_OK {
            return Err(format!("C_GetSlotList: 0x{rv:x}"));
        }
        ids.truncate(n as usize);
        return ids
            .into_iter()
            .map(|id| Slot::try_from(id).map_err(|e| e.to_string()))
            .collect();
    }
    Err("C_GetSlotList: a lista não parou de mudar".into())
}

/// Um campo de texto fixo do PKCS#11 (rótulo, fabricante), sem o
/// preenchimento. A norma pede espaços, mas o SerproID preenche com bytes
/// nulos, que chegariam à janela e ao estado.
pub fn texto_fixo(campo: &str) -> String {
    campo
        .trim_end_matches(|c: char| c == '\0' || c.is_whitespace())
        .to_owned()
}

/// `ancora vigiar <módulo>`: carrega o driver uma vez e, a cada linha na
/// entrada, responde uma linha com os rótulos dos tokens (um array JSON) ou
/// `{"erro": ...}`. Sai quando a entrada fecha.
pub fn vigiar(modulo: &Path) -> Result<(), String> {
    use std::io::{BufRead, Write};
    // SAFETY: carregar o driver roda o construtor dele, e é para isso que
    // este processo existe.
    let biblioteca = unsafe { libloading::Library::new(modulo) }
        .map_err(|e| format!("{}: {e}", modulo.display()))?;
    let pegar: libloading::Symbol<unsafe extern "C" fn(*mut *mut CK_FUNCTION_LIST) -> CK_RV> =
        unsafe { biblioteca.get(b"C_GetFunctionList") }.map_err(|e| e.to_string())?;
    let mut tabela: *mut CK_FUNCTION_LIST = std::ptr::null_mut();
    // SAFETY: C_GetFunctionList só escreve o ponteiro da tabela.
    let rv = unsafe { pegar(&mut tabela) };
    if rv != CKR_OK || tabela.is_null() {
        return Err(format!("C_GetFunctionList: 0x{rv:x}"));
    }
    // SAFETY: a tabela vive enquanto `biblioteca` estiver carregada, até o
    // fim da função.
    let (iniciar, finalizar, listar, info) = unsafe {
        let t = &*tabela;
        (
            t.C_Initialize,
            t.C_Finalize,
            t.C_GetSlotList,
            t.C_GetTokenInfo,
        )
    };
    let (Some(iniciar), Some(finalizar), Some(listar), Some(info)) =
        (iniciar, finalizar, listar, info)
    else {
        return Err("o driver não tem as funções da lista de tokens".into());
    };
    // Como o p11-kit inicializa os módulos: com as travas do sistema.
    let mut argumentos = CK_C_INITIALIZE_ARGS {
        CreateMutex: None,
        DestroyMutex: None,
        LockMutex: None,
        UnlockMutex: None,
        flags: CKF_OS_LOCKING_OK,
        pReserved: std::ptr::null_mut(),
    };
    // SAFETY: os argumentos vivem até o fim da chamada.
    let rv = unsafe { iniciar((&mut argumentos as *mut CK_C_INITIALIZE_ARGS).cast()) };
    if rv != CKR_OK {
        return Err(format!("C_Initialize: 0x{rv:x}"));
    }
    let consultar = || -> Result<Vec<String>, String> {
        let slots = listar_slots(|ids, n| {
            // SAFETY: `ids` é nulo ou tem espaço para `*n` ids.
            unsafe { listar(CK_TRUE, ids, n) }
        })?;
        let mut rotulos = Vec::new();
        for slot in slots {
            // SAFETY: a estrutura é só dados, e o driver a preenche inteira.
            let mut t: CK_TOKEN_INFO = unsafe { std::mem::zeroed() };
            // SAFETY: `t` é uma CK_TOKEN_INFO válida para escrita.
            let rv = unsafe { info(slot.id(), &mut t) };
            // Um slot com token que o driver não reconhece (outro cartão na
            // leitora) não é erro: só não é token dele. O slot livre (o
            // SoftHSM sempre deixa um) tem token sem inicializar.
            if rv == CKR_OK && t.flags & CKF_TOKEN_INITIALIZED != 0 {
                rotulos.push(texto_fixo(&String::from_utf8_lossy(&t.label)));
            }
        }
        Ok(rotulos)
    };
    let mut saida = std::io::stdout().lock();
    for linha in std::io::stdin().lock().lines() {
        if linha.is_err() {
            break;
        }
        let resposta = match consultar() {
            Ok(rotulos) => serde_json::json!(rotulos),
            Err(e) => serde_json::json!({ "erro": e }),
        };
        if writeln!(saida, "{resposta}")
            .and_then(|_| saida.flush())
            .is_err()
        {
            break;
        }
    }
    // SAFETY: fecha o que o C_Initialize abriu.
    unsafe { finalizar(std::ptr::null_mut()) };
    Ok(())
}

/// Uma linha de resposta do `vigiar`: os rótulos, ou o erro.
pub fn ler_resposta(linha: &str) -> Result<Vec<String>, String> {
    let valor: serde_json::Value =
        serde_json::from_str(linha).map_err(|e| format!("resposta da sonda ilegível: {e}"))?;
    match valor {
        serde_json::Value::Array(itens) => Ok(itens
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect()),
        serde_json::Value::Object(o) => Err(o
            .get("erro")
            .and_then(|e| e.as_str())
            .unwrap_or("erro sem motivo")
            .to_owned()),
        _ => Err("resposta da sonda ilegível".into()),
    }
}

pub fn sondar(socket: &Path) -> Result<Vec<String>, String> {
    let (p11, slots) = abrir(socket)?;
    let mut rotulos = Vec::new();
    for slot in slots {
        let token = p11
            .get_token_info(slot)
            .map_err(|e| format!("C_GetTokenInfo: {e}"))?;
        // Um slot livre (o SoftHSM sempre deixa um) tem token sem
        // inicializar, sem rótulo e sem certificado.
        if token.token_initialized() {
            rotulos.push(texto_fixo(token.label()));
        }
    }
    Ok(rotulos)
}

#[cfg(test)]
mod tests {
    use super::*;

    // O RPC do p11-kit: com um buffer não nulo de tamanho zero, responde
    // CKR_BUFFER_TOO_SMALL. Quem pede de novo nesse caso nunca sai.
    fn rpc(ids: &'static [CK_SLOT_ID]) -> impl FnMut(*mut CK_SLOT_ID, &mut CK_ULONG) -> CK_RV {
        move |buffer, n| {
            if buffer.is_null() {
                *n = ids.len() as CK_ULONG;
                return CKR_OK;
            }
            if (*n as usize) < ids.len() || ids.is_empty() {
                *n = ids.len() as CK_ULONG;
                return CKR_BUFFER_TOO_SMALL;
            }
            // SAFETY: o buffer tem espaço para `*n` ids.
            unsafe { std::ptr::copy_nonoverlapping(ids.as_ptr(), buffer, ids.len()) };
            *n = ids.len() as CK_ULONG;
            CKR_OK
        }
    }

    #[test]
    fn texto_fixo_perde_o_preenchimento_de_espacos_e_de_nulos() {
        assert_eq!(texto_fixo("SafeSign IC      "), "SafeSign IC");
        assert_eq!(texto_fixo("SerproID\0\0\0\0"), "SerproID");
        assert_eq!(texto_fixo("Meu token \0 \0"), "Meu token");
        assert_eq!(texto_fixo("\0\0"), "");
    }

    #[test]
    fn a_resposta_do_vigiar_e_a_lista_ou_o_erro() {
        assert_eq!(
            ler_resposta(r#"["GD Burti","SerproID"]"#),
            Ok(vec!["GD Burti".to_owned(), "SerproID".to_owned()])
        );
        assert_eq!(ler_resposta("[]"), Ok(vec![]));
        assert_eq!(
            ler_resposta(r#"{"erro":"C_GetSlotList: 0x30"}"#),
            Err("C_GetSlotList: 0x30".to_owned())
        );
        assert!(ler_resposta("lixo").is_err());
    }

    #[test]
    fn sem_slot_a_lista_vem_vazia_e_sem_laco() {
        assert_eq!(listar_slots(rpc(&[])), Ok(vec![]));
    }

    #[test]
    fn lista_os_slots_do_servidor() {
        let slots = listar_slots(rpc(&[3, 7])).unwrap();
        assert_eq!(slots.iter().map(|s| s.id()).collect::<Vec<_>>(), vec![3, 7]);
    }

    #[test]
    fn um_token_que_chega_no_meio_pede_de_novo_e_desiste_se_nao_para() {
        // A conta diz 1, a lista pede 2: chegou um token. Na segunda volta,
        // a conta já diz 2.
        let mut vez = 0;
        let r = listar_slots(|buffer, n| {
            vez += 1;
            match vez {
                1 => *n = 1,
                2 => {
                    *n = 2;
                    return CKR_BUFFER_TOO_SMALL;
                }
                3 => *n = 2,
                _ => {
                    // SAFETY: o buffer tem espaço para 2 ids.
                    unsafe { std::ptr::copy_nonoverlapping([1, 2].as_ptr(), buffer, 2) };
                    *n = 2;
                }
            }
            CKR_OK
        });
        assert_eq!(r.unwrap().len(), 2);

        let sempre_mudando = listar_slots(|buffer, n| {
            if buffer.is_null() {
                *n = 1;
                CKR_OK
            } else {
                *n = 2;
                CKR_BUFFER_TOO_SMALL
            }
        });
        assert!(sempre_mudando.is_err());
    }
}
