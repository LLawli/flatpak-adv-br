//! A política de supervisão de um driver: o que fazer com o resultado de
//! cada sondagem. Separada do serviço para ser testada sem processos.
//!
//! Um driver só é publicado depois de responder. Um driver travado,
//! publicado, trava todo aplicativo que abre o p11-kit (o SafeNet sem o token
//! dele leva mais de um minuto por enumeração dentro do sandbox), então ele
//! sai da publicação na primeira vez que não responde, e é parado depois de
//! algumas vezes seguidas.

/// Quantas vezes seguidas sem resposta até o driver ser parado.
pub const LIMITE_SEM_RESPOSTA: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sondagem {
    /// Listou os tokens a tempo.
    Respondeu(Vec<String>),
    /// Respondeu com erro.
    Falhou(String),
    /// Não respondeu dentro do limite, e ninguém mais está usando o driver.
    Travou,
    /// Não respondeu dentro do limite enquanto outra conexão usa o driver.
    /// Pode ser uma pessoa digitando o PIN num leitor com teclado, com a
    /// transação do cartão presa: reiniciar mataria essa conexão no meio do
    /// login (medido em tests/e2e/supervisao.sh).
    Ocupado,
    /// O servidor saiu sozinho.
    Caiu,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Estado {
    Iniciando,
    Publicado(Vec<String>),
    ComErro(String),
    Parado(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acao {
    Nenhuma,
    Reiniciar,
    Parar,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Supervisao {
    pub estado: Estado,
    sem_resposta: u32,
}

impl Default for Supervisao {
    fn default() -> Self {
        Supervisao {
            estado: Estado::Iniciando,
            sem_resposta: 0,
        }
    }
}

impl Supervisao {
    /// Vai para o host: respondeu e tem token. Um módulo sem token esconde,
    /// no `p11-kit-server.socket`, todos os módulos depois dele: o iterador do
    /// p11-kit pede a lista de slots com um buffer de tamanho zero, o RPC
    /// responde `CKR_BUFFER_TOO_SMALL`, e ele encerra a iteração inteira
    /// (iter.c, `finish_iterating`). Medido em 2026-10-31: o SafeNet sem token
    /// escondia o GD Burti dos assinadores. Publicar antes não adianta: o
    /// servidor do driver decide os tokens de uma conexão quando ela abre, e
    /// quem conectou sem o token não o vê depois.
    pub fn publicavel(&self) -> bool {
        matches!(&self.estado, Estado::Publicado(tokens) if !tokens.is_empty())
    }

    pub fn parado(&self) -> bool {
        matches!(self.estado, Estado::Parado(_))
    }

    pub fn registrar(&mut self, sondagem: Sondagem) -> Acao {
        if self.parado() {
            return Acao::Nenhuma;
        }
        match sondagem {
            Sondagem::Respondeu(tokens) => {
                self.sem_resposta = 0;
                self.estado = Estado::Publicado(tokens);
                Acao::Nenhuma
            }
            Sondagem::Ocupado => {
                // Sai da publicação, para que um aplicativo novo não fique
                // esperando, mas não reinicia e não conta para parar.
                self.estado =
                    Estado::ComErro("sem resposta enquanto outra conexão usa o driver".into());
                Acao::Nenhuma
            }
            Sondagem::Falhou(erro) => {
                self.sem_resposta = 0;
                self.estado = Estado::ComErro(erro);
                Acao::Nenhuma
            }
            Sondagem::Travou | Sondagem::Caiu => {
                self.sem_resposta += 1;
                let motivo = if sondagem == Sondagem::Caiu {
                    "o servidor do driver caiu"
                } else {
                    "o driver não respondeu"
                };
                if self.sem_resposta >= LIMITE_SEM_RESPOSTA {
                    self.estado =
                        Estado::Parado(format!("{motivo} {LIMITE_SEM_RESPOSTA} vezes seguidas"));
                    Acao::Parar
                } else {
                    self.estado = Estado::ComErro(motivo.to_owned());
                    Acao::Reiniciar
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_e_publicado_depois_de_responder() {
        let mut s = Supervisao::default();
        assert!(!s.publicavel());
        assert_eq!(
            s.registrar(Sondagem::Respondeu(vec!["HUBTESTE".into()])),
            Acao::Nenhuma
        );
        assert_eq!(s.estado, Estado::Publicado(vec!["HUBTESTE".into()]));
    }

    #[test]
    fn sem_token_espetado_responde_mas_fica_fora_da_publicacao() {
        // Saudável, nem parado nem com erro; vai para o host quando o token
        // chegar.
        let mut s = Supervisao::default();
        assert_eq!(s.registrar(Sondagem::Respondeu(vec![])), Acao::Nenhuma);
        assert_eq!(s.estado, Estado::Publicado(vec![]));
        assert!(!s.publicavel());
        s.registrar(Sondagem::Respondeu(vec!["GD Burti".into()]));
        assert!(s.publicavel());
    }

    #[test]
    fn travar_despublica_e_reinicia_na_hora() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec!["T".into()]));
        assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
        assert!(!s.publicavel());
    }

    #[test]
    fn erro_despublica_sem_reiniciar() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec!["T".into()]));
        assert_eq!(
            s.registrar(Sondagem::Falhou("CKR_GENERAL_ERROR".into())),
            Acao::Nenhuma
        );
        assert_eq!(s.estado, Estado::ComErro("CKR_GENERAL_ERROR".into()));
    }

    #[test]
    fn para_depois_do_limite_seguido_e_nao_volta_sozinho() {
        let mut s = Supervisao::default();
        for _ in 1..LIMITE_SEM_RESPOSTA {
            assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
        }
        assert_eq!(s.registrar(Sondagem::Caiu), Acao::Parar);
        assert!(s.parado());
        assert_eq!(
            s.registrar(Sondagem::Respondeu(vec!["T".into()])),
            Acao::Nenhuma
        );
        assert!(s.parado());
    }

    #[test]
    fn ocupado_despublica_sem_reiniciar_nem_contar() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec!["T".into()]));
        for _ in 0..10 {
            assert_eq!(s.registrar(Sondagem::Ocupado), Acao::Nenhuma);
        }
        assert!(!s.publicavel() && !s.parado());
        // A contagem de travadas não andou: ainda são precisas todas.
        for _ in 1..LIMITE_SEM_RESPOSTA {
            assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
        }
        s.registrar(Sondagem::Respondeu(vec!["T".into()]));
        assert!(s.publicavel());
    }

    #[test]
    fn uma_resposta_zera_a_contagem() {
        let mut s = Supervisao::default();
        for _ in 0..10 {
            for _ in 1..LIMITE_SEM_RESPOSTA {
                assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
            }
            s.registrar(Sondagem::Respondeu(vec!["T".into()]));
        }
        assert!(s.publicavel());
    }
}
