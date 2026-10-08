package main

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
)

// Os mesmos casos que o aplicativo usa, em tests/casos-contato.json.
func TestContatoParidadeComOAplicativo(t *testing.T) {
	bruto, err := os.ReadFile("../tests/casos-contato.json")
	if err != nil {
		t.Skipf("sem o arquivo de casos: %v", err)
	}
	var casos struct {
		Validos   []string `json:"validos"`
		Invalidos []string `json:"invalidos"`
	}
	if err := json.Unmarshal(bruto, &casos); err != nil {
		t.Fatalf("casos ilegíveis: %v", err)
	}
	if len(casos.Validos) < 3 || len(casos.Invalidos) < 5 {
		t.Fatalf("poucos casos: %d válidos, %d inválidos", len(casos.Validos), len(casos.Invalidos))
	}
	for _, c := range casos.Validos {
		if _, err := ValidarContato(c); err != nil {
			t.Errorf("%q devia passar: %v", c, err)
		}
	}
	for _, c := range casos.Invalidos {
		if _, err := ValidarContato(c); err == nil {
			t.Errorf("%q devia ser recusado", c)
		}
	}
}

func TestMontarLevaOContatoInteiro(t *testing.T) {
	r := Relato{
		Titulo:   "não assina",
		Mensagem: "meu outro email joao@escritorio.adv.br não deve aparecer",
		Contato:  " fulana@escritorio.adv.br ",
	}
	issue := Montar(r, time.Now())
	if !strings.Contains(issue.Body, "Contato: `fulana@escritorio.adv.br`") {
		t.Errorf("o contato não chegou à issue:\n%s", issue.Body)
	}
	if strings.Contains(issue.Body, "joao@") {
		t.Error("o e-mail da mensagem escapou da sanitização junto com o contato")
	}
}

func TestMontarSemContatoNaoMencionaContato(t *testing.T) {
	issue := Montar(Relato{Titulo: "x", Mensagem: "y"}, time.Now())
	if strings.Contains(issue.Body, "Contato:") {
		t.Errorf("linha de contato num relato sem contato:\n%s", issue.Body)
	}
}

func TestMontarDescartaContatoMalformado(t *testing.T) {
	// O handler já recusa; Montar não pode depender disso, porque a fila
	// reenvia o que foi guardado e uma versão futura pode chamá-lo de outro
	// lugar.
	issue := Montar(Relato{Titulo: "x", Contato: "fulana@exemplo.com\n@LLawli"}, time.Now())
	if strings.Contains(issue.Body, "@LLawli") {
		t.Errorf("contato malformado entrou na issue:\n%s", issue.Body)
	}
}

func TestImpressaoDistingueOContato(t *testing.T) {
	sem := Relato{Titulo: "x", Mensagem: "y", Diagnostico: "z"}
	com := sem
	com.Contato = "fulana@exemplo.com"
	if Impressao(sem) == Impressao(com) {
		t.Error("reenviar só para acrescentar o contato seria tomado por repetido")
	}
}

func TestContatoInvalidoRecusadoAntesDaProva(t *testing.T) {
	s := montarServico(t, GitHub{Base: "http://127.0.0.1:1", Repositorio: "x/y", Token: "t"}, t.TempDir())
	corpo, _ := json.Marshal(Relato{Titulo: "x", Contato: "não é e-mail"})
	pedido := httptest.NewRequest(http.MethodPost, "/api/relato", strings.NewReader(string(corpo)))
	resposta := httptest.NewRecorder()
	s.relato(resposta, pedido)
	if resposta.Code != http.StatusBadRequest {
		t.Fatalf("esperava 400, veio %d: %s", resposta.Code, resposta.Body)
	}
	if !strings.Contains(resposta.Body.String(), "e-mail de contato") {
		t.Errorf("a recusa não diz o que corrigir: %s", resposta.Body)
	}
}
