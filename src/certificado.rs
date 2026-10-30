//! Os certificados de cada token: quem é o titular, quem emitiu e até quando
//! vale. É o que a pessoa precisa ver antes de tentar assinar: um certificado
//! vencido é o problema mais comum, e o navegador só diz "nenhum certificado".
//!
//! `ancora certificados <socket>` lê os objetos públicos de cada token pelo
//! socket do driver, sem PIN, num processo à parte, como a sonda. Do X.509 só
//! interessam três campos, e um leitor DER de poucas linhas evita uma
//! dependência inteira para isso.

use crate::sonda;
use cryptoki::object::{Attribute, AttributeType, ObjectClass};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub rotulo: String,
    pub certificados: Vec<Certificado>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Certificado {
    /// O nome comum do titular. Na ICP-Brasil, `NOME:CPF` ou `RAZÃO:CNPJ`.
    pub titular: String,
    pub emissor: String,
    pub validade: Data,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Data {
    pub ano: u16,
    pub mes: u8,
    pub dia: u8,
}

/// A partir de quantos dias antes do fim a validade vira aviso: o tempo de
/// pedir a renovação e de ela chegar.
pub const AVISO_DIAS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prazo {
    Valido,
    /// Vence em tantos dias (0 é hoje).
    Vencendo(i64),
    Vencido,
}

impl Data {
    pub fn hoje() -> Data {
        let agora = gtk::glib::DateTime::now_local().expect("relógio do sistema");
        Data {
            ano: agora.year() as u16,
            mes: agora.month() as u8,
            dia: agora.day_of_month() as u8,
        }
    }

    /// Dias desde 01/01/1970, pelo algoritmo `days_from_civil` de Howard
    /// Hinnant.
    fn dias(self) -> i64 {
        let (d, m) = (i64::from(self.dia), i64::from(self.mes));
        let a = i64::from(self.ano) - i64::from(m <= 2);
        let era = a.div_euclid(400);
        let ano_da_era = a - era * 400;
        let dia_do_ano = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
        let dia_da_era = ano_da_era * 365 + ano_da_era / 4 - ano_da_era / 100 + dia_do_ano;
        era * 146_097 + dia_da_era - 719_468
    }

    pub fn prazo(self, hoje: Data) -> Prazo {
        match self.dias() - hoje.dias() {
            d if d < 0 => Prazo::Vencido,
            d if d <= AVISO_DIAS => Prazo::Vencendo(d),
            _ => Prazo::Valido,
        }
    }
}

impl std::fmt::Display for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:02}/{:02}/{}", self.dia, self.mes, self.ano)
    }
}

/// `ancora certificados` num subprocesso, desistindo depois de `limite`.
pub fn listar_com_limite(socket: &Path, limite: Duration) -> Result<Vec<Token>, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("certificados").arg(socket);
    let saida = crate::processo::executar(cmd, b"", limite)?;
    if !saida.status.success() {
        return Err(String::from_utf8_lossy(&saida.stderr).trim().to_owned());
    }
    serde_json::from_slice(&saida.stdout).map_err(|e| e.to_string())
}

/// Os certificados de cada token inicializado, lidos sem login.
pub fn listar(socket: &Path) -> Result<Vec<Token>, String> {
    let (p11, slots) = sonda::abrir(socket)?;
    let mut tokens = Vec::new();
    for slot in slots {
        let info = p11
            .get_token_info(slot)
            .map_err(|e| format!("C_GetTokenInfo: {e}"))?;
        if !info.token_initialized() {
            continue;
        }
        let sessao = p11
            .open_ro_session(slot)
            .map_err(|e| format!("C_OpenSession: {e}"))?;
        let objetos = sessao
            .find_objects(&[Attribute::Class(ObjectClass::CERTIFICATE)])
            .map_err(|e| format!("C_FindObjects: {e}"))?;
        let mut certificados = Vec::new();
        for objeto in objetos {
            let atributos = sessao
                .get_attributes(objeto, &[AttributeType::Value])
                .map_err(|e| format!("C_GetAttributeValue: {e}"))?;
            for a in atributos {
                if let Attribute::Value(der) = a
                    && let Some(c) = ler(&der)
                {
                    certificados.push(c);
                }
            }
        }
        // Alguns drivers mostram o mesmo certificado mais de uma vez.
        certificados.sort();
        certificados.dedup();
        tokens.push(Token {
            rotulo: info.label().trim_end().to_owned(),
            certificados,
        });
    }
    Ok(tokens)
}

/// O titular, o emissor e o fim da validade de um certificado X.509 em DER.
pub fn ler(der: &[u8]) -> Option<Certificado> {
    let certificado = Der(der).esperar(SEQUENCIA)?;
    let tbs = Der(certificado).esperar(SEQUENCIA)?;
    let mut campos = Der(tbs);
    let mut atual = campos.proximo()?;
    // A versão é opcional ([0] explícito) e vem antes do número de série.
    if atual.0 == VERSAO {
        atual = campos.proximo()?;
    }
    if atual.0 != INTEIRO {
        return None;
    }
    campos.esperar(SEQUENCIA)?; // o algoritmo da assinatura
    let emissor = nome(campos.esperar(SEQUENCIA)?);
    let mut validade = Der(campos.esperar(SEQUENCIA)?);
    validade.proximo()?; // o começo da validade
    let (tag, fim) = validade.proximo()?;
    let validade = data(tag, fim)?;
    let titular = nome(campos.esperar(SEQUENCIA)?);
    Some(Certificado {
        titular,
        emissor,
        validade,
    })
}

/// Separa o documento do nome, no formato da ICP-Brasil: `NOME:CPF` ou
/// `RAZÃO:CNPJ`. Fora desse formato, o nome fica inteiro.
pub fn titular(nome_comum: &str) -> (&str, Option<String>) {
    let Some((nome, numero)) = nome_comum.rsplit_once(':') else {
        return (nome_comum, None);
    };
    if !numero.bytes().all(|b| b.is_ascii_digit()) {
        return (nome_comum, None);
    }
    let documento = match numero.len() {
        11 => format!(
            "CPF {}.{}.{}-{}",
            &numero[..3],
            &numero[3..6],
            &numero[6..9],
            &numero[9..]
        ),
        14 => format!(
            "CNPJ {}.{}.{}/{}-{}",
            &numero[..2],
            &numero[2..5],
            &numero[5..8],
            &numero[8..12],
            &numero[12..]
        ),
        _ => return (nome_comum, None),
    };
    (nome, Some(documento))
}

const SEQUENCIA: u8 = 0x30;
const CONJUNTO: u8 = 0x31;
const INTEIRO: u8 = 0x02;
const OID: u8 = 0x06;
const VERSAO: u8 = 0xa0;
const UTC_TIME: u8 = 0x17;
const GENERALIZED_TIME: u8 = 0x18;

/// 2.5.4.3 e 2.5.4.10: o nome comum e a organização.
const NOME_COMUM: &[u8] = &[0x55, 0x04, 0x03];
const ORGANIZACAO: &[u8] = &[0x55, 0x04, 0x0a];

/// Um leitor de TLV em DER, um elemento por vez.
struct Der<'a>(&'a [u8]);

impl<'a> Der<'a> {
    fn proximo(&mut self) -> Option<(u8, &'a [u8])> {
        let (&tag, resto) = self.0.split_first()?;
        let (&primeiro, mut resto) = resto.split_first()?;
        let tamanho = if primeiro < 0x80 {
            usize::from(primeiro)
        } else {
            let n = usize::from(primeiro & 0x7f);
            if n == 0 || n > 4 || resto.len() < n {
                return None;
            }
            let t = resto[..n]
                .iter()
                .fold(0usize, |a, &b| (a << 8) | usize::from(b));
            resto = &resto[n..];
            t
        };
        if resto.len() < tamanho {
            return None;
        }
        let (conteudo, depois) = resto.split_at(tamanho);
        self.0 = depois;
        Some((tag, conteudo))
    }

    fn esperar(&mut self, tag: u8) -> Option<&'a [u8]> {
        let (t, conteudo) = self.proximo()?;
        (t == tag).then_some(conteudo)
    }
}

/// O nome comum de um `Name`, ou a organização, se não houver.
fn nome(name: &[u8]) -> String {
    let mut comum = None;
    let mut organizacao = None;
    let mut conjuntos = Der(name);
    while let Some(conjunto) = conjuntos.esperar(CONJUNTO) {
        let mut atributos = Der(conjunto);
        while let Some(atributo) = atributos.esperar(SEQUENCIA) {
            let mut partes = Der(atributo);
            let (Some(oid), Some((tag, valor))) = (partes.esperar(OID), partes.proximo()) else {
                continue;
            };
            if oid == NOME_COMUM {
                comum = texto(tag, valor);
            } else if oid == ORGANIZACAO {
                organizacao = texto(tag, valor);
            }
        }
    }
    comum.or(organizacao).unwrap_or_default()
}

fn texto(tag: u8, valor: &[u8]) -> Option<String> {
    match tag {
        // UTF8String, PrintableString, IA5String
        0x0c | 0x13 | 0x16 => Some(String::from_utf8_lossy(valor).into_owned()),
        // TeletexString: na prática, Latin-1
        0x14 => Some(valor.iter().map(|&b| char::from(b)).collect()),
        // BMPString
        0x1e => {
            let unidades: Vec<u16> = valor
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&c| u16::from_be_bytes(c))
                .collect();
            Some(String::from_utf16_lossy(&unidades))
        }
        _ => None,
    }
}

fn data(tag: u8, valor: &[u8]) -> Option<Data> {
    let texto = std::str::from_utf8(valor).ok()?;
    let numero = |de: usize, ate: usize| texto.get(de..ate)?.parse::<u16>().ok();
    let (ano, resto) = match tag {
        // AAMMDD: de 1950 a 2049
        UTC_TIME => {
            let aa = numero(0, 2)?;
            (if aa < 50 { 2000 + aa } else { 1900 + aa }, 2)
        }
        GENERALIZED_TIME => (numero(0, 4)?, 4),
        _ => return None,
    };
    let mes = u8::try_from(numero(resto, resto + 2)?).ok()?;
    let dia = u8::try_from(numero(resto + 2, resto + 4)?).ok()?;
    ((1..=12).contains(&mes) && (1..=31).contains(&dia)).then_some(Data { ano, mes, dia })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Gerados com o openssl: um certificado de pessoa física, emitido por uma
    // AC de teste, com acento no nome; e um autoassinado de empresa que vence
    // depois de 2049, quando a validade passa a GeneralizedTime.
    const JOSE: &[u8] = include_bytes!("../tests/dados/jose.der");
    const LONGE: &[u8] = include_bytes!("../tests/dados/longe.der");

    #[test]
    fn le_titular_emissor_e_validade() {
        assert_eq!(
            ler(JOSE),
            Some(Certificado {
                titular: "JOSÉ DA SILVA:12345678900".into(),
                emissor: "AC TESTE v5".into(),
                validade: Data {
                    ano: 2027,
                    mes: 10,
                    dia: 29
                },
            })
        );
    }

    #[test]
    fn le_validade_depois_de_2049() {
        let c = ler(LONGE).unwrap();
        assert_eq!(c.titular, "EMPRESA LTDA:12345678000190");
        assert_eq!(c.emissor, c.titular);
        assert_eq!(
            c.validade,
            Data {
                ano: 2051,
                mes: 1,
                dia: 1
            }
        );
    }

    #[test]
    fn recusa_o_que_nao_e_certificado() {
        assert_eq!(ler(b""), None);
        assert_eq!(ler(&[0x30, 0x82, 0xff]), None);
        assert_eq!(ler(&JOSE[..JOSE.len() / 2]), None);
        // Um tamanho maior que o resto não pode ler além do fim.
        assert_eq!(ler(&[0x30, 0x84, 0xff, 0xff, 0xff, 0xff]), None);
    }

    #[test]
    fn separa_cpf_e_cnpj_do_nome() {
        assert_eq!(
            titular("JOSÉ DA SILVA:12345678900"),
            ("JOSÉ DA SILVA", Some("CPF 123.456.789-00".into()))
        );
        assert_eq!(
            titular("EMPRESA LTDA:12345678000190"),
            ("EMPRESA LTDA", Some("CNPJ 12.345.678/0001-90".into()))
        );
        assert_eq!(titular("Teste A3"), ("Teste A3", None));
        assert_eq!(titular("SERVIDOR:443"), ("SERVIDOR:443", None));
        assert_eq!(titular("NOME:1234567890a"), ("NOME:1234567890a", None));
    }

    fn d(ano: u16, mes: u8, dia: u8) -> Data {
        Data { ano, mes, dia }
    }

    #[test]
    fn conta_os_dias_ate_o_fim_da_validade() {
        assert_eq!(d(1970, 1, 1).dias(), 0);
        assert_eq!(d(2000, 3, 1).dias() - d(2000, 2, 28).dias(), 2);
        assert_eq!(d(2027, 1, 1).dias() - d(2026, 12, 31).dias(), 1);
        let hoje = d(2026, 10, 30);
        assert_eq!(d(2026, 10, 29).prazo(hoje), Prazo::Vencido);
        assert_eq!(d(2026, 10, 30).prazo(hoje), Prazo::Vencendo(0));
        assert_eq!(d(2026, 11, 29).prazo(hoje), Prazo::Vencendo(30));
        assert_eq!(d(2026, 11, 30).prazo(hoje), Prazo::Valido);
    }

    #[test]
    fn mostra_a_data_como_no_brasil() {
        let d = Data {
            ano: 2027,
            mes: 3,
            dia: 9,
        };
        assert_eq!(d.to_string(), "09/03/2027");
    }
}
