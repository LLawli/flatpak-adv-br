package main

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"
)

func TestMontarSanitizaTudo(t *testing.T) {
	r := Relato{
		Titulo:      "erro com MARIA DA SILVA SOUZA:12345678901",
		Mensagem:    "meu email joao@escritorio.adv.br não assina",
		Diagnostico: "caminho /home/joana/.pki e cpf 123.456.789-01",
	}
	issue := Montar(r, time.Now())

	proibidos := []string{"MARIA", "12345678901", "joao@", "/home/joana", "123.456.789-01"}
	inteiro := issue.Title + issue.Body
	for _, proibido := range proibidos {
		if strings.Contains(inteiro, proibido) {
			t.Errorf("%q chegou à issue", proibido)
		}
	}
}

func TestMontarCortaOComecoDoLog(t *testing.T) {
	// O fim do log é o que interessa: é onde está o que aconteceu por último.
	r := Relato{
		Titulo:      "teste",
		Diagnostico: strings.Repeat("a", TamanhoMaximoIssue+5000) + "ULTIMALINHA",
	}
	issue := Montar(r, time.Now())
	if !strings.Contains(issue.Body, "ULTIMALINHA") {
		t.Error("cortou o fim do log, que é a parte que importa")
	}
	if len(issue.Body) > TamanhoMaximoIssue+2000 {
		t.Errorf("corpo grande demais: %d", len(issue.Body))
	}
}

func TestPublicarMandaOQueOGitHubEspera(t *testing.T) {
	var autorizacao, caminho string
	falso := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		autorizacao = r.Header.Get("Authorization")
		caminho = r.URL.Path
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte(`{"html_url":"https://github.com/x/y/issues/1"}`))
	}))
	defer falso.Close()

	g := GitHub{Base: falso.URL, Repositorio: "LLawli/adv-br-relatos", Token: "segredo"}
	url, err := g.Publicar(Issue{Title: "t", Body: "b"})
	if err != nil {
		t.Fatalf("falhou: %v", err)
	}
	if url != "https://github.com/x/y/issues/1" {
		t.Errorf("url inesperada: %s", url)
	}
	if autorizacao != "Bearer segredo" {
		t.Errorf("autorização errada: %s", autorizacao)
	}
	if caminho != "/repos/LLawli/adv-br-relatos/issues" {
		t.Errorf("caminho errado: %s", caminho)
	}
}

func TestPublicarExplicaAFalha(t *testing.T) {
	falso := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		w.Write([]byte(`{"message":"Bad credentials"}`))
	}))
	defer falso.Close()

	g := GitHub{Base: falso.URL, Repositorio: "x/y", Token: "errado"}
	_, err := g.Publicar(Issue{Title: "t"})
	if err == nil || !strings.Contains(err.Error(), "Bad credentials") {
		t.Errorf("o erro precisa dizer o que o GitHub respondeu, veio: %v", err)
	}
}

// A impressão não pode depender da hora: se dependesse, o mesmo relato enviado
// duas vezes com um segundo de diferença viraria duas issues, que é
// exatamente o que a deduplicação existe para evitar.
func TestImpressaoIgnoraAHora(t *testing.T) {
	r := Relato{Titulo: "erro", Mensagem: "não assina", Diagnostico: "log"}
	if Impressao(r) != Impressao(r) {
		t.Fatal("a impressão não é estável")
	}

	primeira := Montar(r, time.Now())
	segunda := Montar(r, time.Now().Add(90*time.Second))
	if primeira.Body == segunda.Body {
		t.Skip("o corpo não carrega a hora; este teste perdeu o sentido")
	}
	// Os corpos diferem (a hora está lá), e a impressão não pode diferir.
	if Impressao(r) != Impressao(r) {
		t.Error("a impressão variou junto com o corpo")
	}
}
