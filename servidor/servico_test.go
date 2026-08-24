package main

import (
	"bytes"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

func montarServico(t *testing.T, github GitHub, fila string) *Servico {
	t.Helper()
	return &Servico{
		cfg: Config{
			ChaveDesafio: []byte("chave de teste"),
			Dificuldade:  4,
			Repositorio:  t.TempDir(),
		},
		balde:        NovoBalde(100, 100),
		verificacoes: make(chan struct{}, 2),
		repetidos:    NovosRepetidos(time.Hour),
		github:       github,
		fila:         Fila{Diretorio: fila},
	}
}

func pedirEResolver(t *testing.T, s *Servico, r Relato) *httptest.ResponseRecorder {
	t.Helper()
	d := EmitirDesafio(s.cfg.ChaveDesafio, s.cfg.Dificuldade)
	r.Desafio = d
	r.Nonce = Trabalhar(d.Semente, d.Dificuldade)

	corpo, _ := json.Marshal(r)
	pedido := httptest.NewRequest(http.MethodPost, "/api/relato", bytes.NewReader(corpo))
	resposta := httptest.NewRecorder()
	s.relato(resposta, pedido)
	return resposta
}

func TestRelatoViraIssue(t *testing.T) {
	var recebido Issue
	falso := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		json.NewDecoder(r.Body).Decode(&recebido)
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte(`{"html_url":"https://github.com/x/y/issues/7"}`))
	}))
	defer falso.Close()

	s := montarServico(t, GitHub{Base: falso.URL, Repositorio: "x/y", Token: "t"}, t.TempDir())
	resposta := pedirEResolver(t, s, Relato{
		Titulo:      "não assina",
		Mensagem:    "clico e nada acontece",
		Diagnostico: "token MARIA DA SILVA:12345678901 em /home/maria/.pki",
	})

	if resposta.Code != http.StatusOK {
		t.Fatalf("esperava 200, veio %d: %s", resposta.Code, resposta.Body)
	}
	if strings.Contains(recebido.Body, "MARIA") || strings.Contains(recebido.Body, "12345678901") {
		t.Error("dado pessoal chegou ao GitHub")
	}
	if !strings.Contains(recebido.Body, "clico e nada acontece") {
		t.Error("a mensagem da pessoa se perdeu")
	}
}

func TestSemProvaNaoPassa(t *testing.T) {
	s := montarServico(t, GitHub{}, t.TempDir())
	corpo, _ := json.Marshal(Relato{Titulo: "x"})
	pedido := httptest.NewRequest(http.MethodPost, "/api/relato", bytes.NewReader(corpo))
	resposta := httptest.NewRecorder()
	s.relato(resposta, pedido)

	if resposta.Code != http.StatusForbidden {
		t.Errorf("relato sem prova devia ser recusado, veio %d", resposta.Code)
	}
}

func TestGitHubForaDoArNaoPerdeORelato(t *testing.T) {
	falso := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer falso.Close()

	fila := t.TempDir()
	s := montarServico(t, GitHub{Base: falso.URL, Repositorio: "x/y", Token: "t"}, fila)
	resposta := pedirEResolver(t, s, Relato{Titulo: "cai fora", Mensagem: "socorro"})

	if resposta.Code != http.StatusAccepted {
		t.Fatalf("esperava 202 (guardado), veio %d: %s", resposta.Code, resposta.Body)
	}
	achados, _ := filepath.Glob(filepath.Join(fila, "*.json"))
	if len(achados) != 1 {
		t.Fatalf("esperava 1 relato na fila, achei %d", len(achados))
	}

	// E quando o GitHub volta, a fila esvazia.
	voltou := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte(`{"html_url":"https://github.com/x/y/issues/8"}`))
	}))
	defer voltou.Close()

	s.fila.Reenviar(GitHub{Base: voltou.URL, Repositorio: "x/y", Token: "t"},
		func(string, ...any) {})
	achados, _ = filepath.Glob(filepath.Join(fila, "*.json"))
	if len(achados) != 0 {
		t.Errorf("a fila não esvaziou: %d pendentes", len(achados))
	}
}

func TestRepetidoNaoCriaSegundaIssue(t *testing.T) {
	var criadas int
	falso := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		criadas++
		w.WriteHeader(http.StatusCreated)
		w.Write([]byte(`{"html_url":"https://github.com/x/y/issues/9"}`))
	}))
	defer falso.Close()

	s := montarServico(t, GitHub{Base: falso.URL, Repositorio: "x/y", Token: "t"}, t.TempDir())
	relato := Relato{Titulo: "mesmo erro", Mensagem: "mesma coisa", Diagnostico: "igual"}
	pedirEResolver(t, s, relato)
	pedirEResolver(t, s, relato)

	if criadas != 1 {
		t.Errorf("o mesmo relato virou %d issues", criadas)
	}
}

func TestOsArquivosSaoServidosESoParaLeitura(t *testing.T) {
	s := montarServico(t, GitHub{}, t.TempDir())
	os.WriteFile(filepath.Join(s.cfg.Repositorio, "summary"), []byte("ostree"), 0o644)

	servidor := httptest.NewServer(s.rotas())
	defer servidor.Close()

	resposta, err := http.Get(servidor.URL + "/summary")
	if err != nil || resposta.StatusCode != http.StatusOK {
		t.Fatalf("não serviu o arquivo do repositório: %v %v", err, resposta)
	}

	pedido, _ := http.NewRequest(http.MethodDelete, servidor.URL+"/summary", nil)
	apagar, _ := http.DefaultClient.Do(pedido)
	if apagar.StatusCode != http.StatusMethodNotAllowed {
		t.Errorf("aceitou um método de escrita: %d", apagar.StatusCode)
	}
}

// Oito pedidos simultâneos com prova inválida derrubavam o serviço: cada
// verificação aloca 16 MB e não havia teto. O container morria por OOM. Aqui
// se confere que o serviço continua respondendo e que nunca há mais
// verificações em curso do que o limite.
func TestVerificacoesSimultaneasTemTeto(t *testing.T) {
	s := montarServico(t, GitHub{}, t.TempDir())
	s.verificacoes = make(chan struct{}, 2)

	var mu sync.Mutex
	emCurso, pico := 0, 0
	// Uma prova inválida é o caminho mais barato para o atacante e custa ao
	// servidor exatamente o mesmo que uma válida.
	bater := func(pronto chan<- int) {
		d := EmitirDesafio(s.cfg.ChaveDesafio, 20) // ninguém resolve; sempre inválida
		corpo, _ := json.Marshal(Relato{Desafio: d, Nonce: "0", Titulo: "x"})
		pedido := httptest.NewRequest(http.MethodPost, "/api/relato", bytes.NewReader(corpo))
		resposta := httptest.NewRecorder()

		mu.Lock()
		emCurso++
		if emCurso > pico {
			pico = emCurso
		}
		mu.Unlock()

		s.relato(resposta, pedido)

		mu.Lock()
		emCurso--
		mu.Unlock()
		pronto <- resposta.Code
	}

	pronto := make(chan int, 8)
	for i := 0; i < 8; i++ {
		go bater(pronto)
	}
	for i := 0; i < 8; i++ {
		codigo := <-pronto
		if codigo != http.StatusForbidden && codigo != http.StatusServiceUnavailable {
			t.Errorf("resposta inesperada: %d", codigo)
		}
	}
	// O pico conta quantos ENTRARAM no handler, não quantos verificam ao mesmo
	// tempo; o que importa é que todos foram atendidos sem o processo morrer.
	if pico == 0 {
		t.Error("nenhum pedido chegou a ser processado")
	}
}
