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
    pub fn publicado(&self) -> bool {
        matches!(self.estado, Estado::Publicado(_))
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
        assert!(!s.publicado());
        assert_eq!(
            s.registrar(Sondagem::Respondeu(vec!["HUBTESTE".into()])),
            Acao::Nenhuma
        );
        assert_eq!(s.estado, Estado::Publicado(vec!["HUBTESTE".into()]));
    }

    #[test]
    fn sem_token_espetado_continua_publicado() {
        // Driver que responde com zero tokens está saudável: o token pode ser
        // espetado depois.
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec![]));
        assert!(s.publicado());
    }

    #[test]
    fn travar_despublica_e_reinicia_na_hora() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec![]));
        assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
        assert!(!s.publicado());
    }

    #[test]
    fn erro_despublica_sem_reiniciar() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec![]));
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
        assert_eq!(s.registrar(Sondagem::Respondeu(vec![])), Acao::Nenhuma);
        assert!(s.parado());
    }

    #[test]
    fn ocupado_despublica_sem_reiniciar_nem_contar() {
        let mut s = Supervisao::default();
        s.registrar(Sondagem::Respondeu(vec![]));
        for _ in 0..10 {
            assert_eq!(s.registrar(Sondagem::Ocupado), Acao::Nenhuma);
        }
        assert!(!s.publicado() && !s.parado());
        // A contagem de travadas não andou: ainda são precisas todas.
        for _ in 1..LIMITE_SEM_RESPOSTA {
            assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
        }
        s.registrar(Sondagem::Respondeu(vec![]));
        assert!(s.publicado());
    }

    #[test]
    fn uma_resposta_zera_a_contagem() {
        let mut s = Supervisao::default();
        for _ in 0..10 {
            for _ in 1..LIMITE_SEM_RESPOSTA {
                assert_eq!(s.registrar(Sondagem::Travou), Acao::Reiniciar);
            }
            s.registrar(Sondagem::Respondeu(vec![]));
        }
        assert!(s.publicado());
    }
}
