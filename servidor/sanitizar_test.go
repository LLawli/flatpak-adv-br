package main

import "strings"

import "testing"

func TestSanitizar(t *testing.T) {
	casos := []struct {
		nome     string
		entrada  string
		proibido string
	}{
		{"rótulo de token ICP-Brasil", "token: MARIA DA SILVA SOUZA:12345678901", "MARIA"},
		{"CPF com pontuação", "titular 123.456.789-01 no slot 3", "123.456.789-01"},
		{"CPF sem pontuação", "serial=12345678901 emitido", "12345678901"},
		{"CNPJ", "empresa 12.345.678/0001-99 aqui", "12.345.678/0001-99"},
		{"e-mail", "contato joao.silva@escritorio.adv.br ok", "joao.silva@"},
		{"home comum", "erro em /home/joana/.mozilla/firefox", "/home/joana"},
		{"home atômico", "erro em /var/home/joana/.pki", "/var/home/joana"},
		{"impressão digital", "sha1 da61ebbc1b3f8a2f1c0e9a4b6d8f2e1a3c5b7d9e ok",
			"da61ebbc1b3f8a2f1c0e9a4b6d8f2e1a3c5b7d9e"},
	}

	for _, caso := range casos {
		saida := Sanitizar(caso.entrada)
		if strings.Contains(saida, caso.proibido) {
			t.Errorf("%s: %q sobreviveu em %q", caso.nome, caso.proibido, saida)
		}
	}
}

// O texto útil não pode desaparecer junto: um log inteiro virando [TITULAR]
// seria uma sanitização perfeita e um relato inútil.
func TestSanitizarPreservaODiagnostico(t *testing.T) {
	entrada := "18:20:45 pkcs11: C_GetSlotList devolveu 0x30\n" +
		"série do host 0.25, do pacote 0.26"
	saida := Sanitizar(entrada)
	for _, precisa := range []string{"C_GetSlotList", "0x30", "0.25", "0.26", "pkcs11"} {
		if !strings.Contains(saida, precisa) {
			t.Errorf("a limpeza comeu %q: %q", precisa, saida)
		}
	}
}
