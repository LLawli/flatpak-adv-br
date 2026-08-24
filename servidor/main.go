// O serviço do adv-br: serve o repositório Flatpak e recebe relatos de erro.
//
// Um binário, sem banco de dados. O que ele guarda em disco é o repositório
// (que chega por rsync, não por aqui) e a fila de relatos que o GitHub recusou
// na hora.
//
// Por que existe um serviço no meio, em vez de o aplicativo falar direto com o
// GitHub: para criar uma issue é preciso um token, e um token embutido num
// aplicativo distribuído não é um segredo. Aqui ele fica no servidor, e o que
// o aplicativo manda é um texto já sanitizado, com uma prova de trabalho.
package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"os"
	"strconv"
	"strings"
	"time"
)

type Config struct {
	Endereco     string
	Repositorio  string // diretório do repositório Flatpak, servido como arquivos
	RepoIssues   string // dono/nome no GitHub
	Token        string
	Fila         string
	Dificuldade  int
	PorHora      float64
	Teto         float64
	Simultaneas  int
	ChaveDesafio []byte
}

func ambiente(nome, padrao string) string {
	// Segredo por arquivo, e não por variável: o conteúdo de uma variável
	// aparece em `docker inspect`, no `ps` e em qualquer despejo de ambiente.
	if caminho := os.Getenv(nome + "_FILE"); caminho != "" {
		bruto, err := os.ReadFile(caminho)
		if err == nil {
			return strings.TrimSpace(string(bruto))
		}
		log.Printf("não consegui ler %s: %v", caminho, err)
	}
	if valor := os.Getenv(nome); valor != "" {
		return valor
	}
	return padrao
}

func numero(nome string, padrao float64) float64 {
	if valor, err := strconv.ParseFloat(ambiente(nome, ""), 64); err == nil {
		return valor
	}
	return padrao
}

func carregar() Config {
	chave := ambiente("ADVBR_CHAVE_DESAFIO", "")
	if chave == "" {
		// Sem chave configurada, uma por execução. Os desafios em voo morrem
		// no reinício, o que custa uma tentativa a quem estava escrevendo.
		bruto := make([]byte, 32)
		if _, err := io.ReadFull(randomico{}, bruto); err != nil {
			log.Fatalf("sem fonte de aleatoriedade: %v", err)
		}
		chave = string(bruto)
	}
	return Config{
		Endereco:    ambiente("ADVBR_ENDERECO", "127.0.0.1:8080"),
		Repositorio: ambiente("ADVBR_REPOSITORIO", "/var/lib/adv-br/repo"),
		RepoIssues:  ambiente("ADVBR_REPO_ISSUES", "LLawli/adv-br-relatos"),
		Token:       ambiente("ADVBR_TOKEN_GITHUB", ""),
		Fila:        ambiente("ADVBR_FILA", "/var/lib/adv-br/fila"),
		Dificuldade: int(numero("ADVBR_DIFICULDADE", DificuldadePadrao)),
		PorHora:     numero("ADVBR_RELATOS_POR_HORA", 5),
		// Duas verificações simultâneas custam 32 MB no pico. Com o
		// interpretador e o resto, cabe folgado em 96 MB de limite.
		Simultaneas:  int(numero("ADVBR_VERIFICACOES_SIMULTANEAS", 2)),
		Teto:         numero("ADVBR_RELATOS_TETO", 5),
		ChaveDesafio: []byte(chave),
	}
}

type Servico struct {
	cfg       Config
	balde     *Balde
	repetidos *Repetidos
	github    GitHub
	fila      Fila

	// Quantas provas de trabalho se verificam ao mesmo tempo.
	//
	// Sem este limite o serviço cai com um pedido de curl repetido: cada
	// verificação aloca os mesmos 16 MB que o cliente gastou para resolver, e
	// oito ao mesmo tempo passam de 128 MB. Medido: o container foi morto pelo
	// OOM killer no oitavo pedido simultâneo, com provas INVÁLIDAS, que são as
	// mais baratas de mandar e custam o mesmo para conferir.
	//
	// O teto vale para o pior caso, não para o normal: quem relata um problema
	// manda um pedido, não oito.
	verificacoes chan struct{}
}

func (s *Servico) desafio(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "método não permitido", http.StatusMethodNotAllowed)
		return
	}
	responder(w, http.StatusOK, EmitirDesafio(s.cfg.ChaveDesafio, s.cfg.Dificuldade))
}

func (s *Servico) relato(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "método não permitido", http.StatusMethodNotAllowed)
		return
	}

	var pedido Relato
	corpo := http.MaxBytesReader(w, r.Body, TamanhoMaximoCorpo)
	if err := json.NewDecoder(corpo).Decode(&pedido); err != nil {
		responder(w, http.StatusBadRequest, mapa{"erro": "não entendi o pedido"})
		return
	}

	// A verificação é o único ponto caro daqui, em memória e em CPU. Quem
	// chega além do limite espera; quem espera demais recebe um "tente de
	// novo", que é melhor que o serviço inteiro cair.
	select {
	case s.verificacoes <- struct{}{}:
		defer func() { <-s.verificacoes }()
	case <-time.After(15 * time.Second):
		responder(w, http.StatusServiceUnavailable,
			mapa{"erro": "servidor ocupado; tente de novo em instantes"})
		return
	}

	// A prova vem antes do limite por endereço: ela é o que encarece o abuso
	// vindo de muitos endereços, que é justamente o caso em que o limite por
	// endereço não ajuda.
	if err := ValidarProva(s.cfg.ChaveDesafio, pedido.Desafio, pedido.Nonce, time.Now()); err != nil {
		codigo := http.StatusForbidden
		if errors.Is(err, ErrExpirado) {
			codigo = http.StatusGone
		}
		responder(w, codigo, mapa{"erro": err.Error()})
		return
	}

	if !s.balde.Permite(quemChama(r)) {
		responder(w, http.StatusTooManyRequests,
			mapa{"erro": "muitos relatos deste endereço; tente mais tarde"})
		return
	}

	issue := Montar(pedido, time.Now())
	if !s.repetidos.Novo(Impressao(pedido)) {
		// Não é erro: o relato chegou, e alguém já o mandou. Dizer "deu certo"
		// é honesto, e evita a pessoa tentar de novo achando que falhou.
		responder(w, http.StatusOK, mapa{"situacao": "repetido"})
		return
	}

	url, err := s.github.Publicar(issue)
	if err != nil {
		log.Printf("github recusou: %v", err)
		if erroFila := s.fila.Guardar(issue); erroFila != nil {
			log.Printf("e a fila falhou: %v", erroFila)
			responder(w, http.StatusBadGateway, mapa{"erro": "não consegui registrar agora"})
			return
		}
		// Para quem relatou, guardado é tão bom quanto publicado.
		responder(w, http.StatusAccepted, mapa{"situacao": "guardado"})
		return
	}
	responder(w, http.StatusOK, mapa{"situacao": "publicado", "url": url})
}

func (s *Servico) saude(w http.ResponseWriter, r *http.Request) {
	pendentes, _ := s.fila.Pendentes()
	responder(w, http.StatusOK, mapa{
		"situacao":  "de pé",
		"pendentes": len(pendentes),
	})
}

type mapa map[string]any

func responder(w http.ResponseWriter, codigo int, corpo any) {
	w.Header().Set("Content-Type", "application/json; charset=utf-8")
	w.WriteHeader(codigo)
	json.NewEncoder(w).Encode(corpo)
}

// quemChama devolve o endereço de quem fez o pedido, respeitando o cabeçalho
// que o proxy põe. O serviço só escuta em localhost, então confiar no
// X-Forwarded-For aqui é confiar no proxy, que é quem está na frente.
//
// O Caddy manda esse cabeçalho por conta própria em todo reverse_proxy; com
// nginx é preciso configurar. Se um dia o serviço passar a escutar em endereço
// público, este cabeçalho vira mentira que qualquer cliente pode contar, e o
// limite por endereço deixa de valer.
func quemChama(r *http.Request) string {
	if repassado := r.Header.Get("X-Forwarded-For"); repassado != "" {
		return strings.TrimSpace(strings.Split(repassado, ",")[0])
	}
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err != nil {
		return r.RemoteAddr
	}
	return host
}

func (s *Servico) rotas() *http.ServeMux {
	mux := http.NewServeMux()
	mux.HandleFunc("/api/desafio", s.desafio)
	mux.HandleFunc("/api/relato", s.relato)
	mux.HandleFunc("/api/saude", s.saude)

	// O repositório Flatpak e os componentes são arquivos estáticos. É o
	// mesmo processo por decisão: um container, um volume, um lugar para
	// olhar.
	arquivos := http.FileServer(http.Dir(s.cfg.Repositorio))
	mux.Handle("/", somenteLeitura(arquivos))
	return mux
}

// somenteLeitura recusa qualquer método que não seja leitura, antes de o
// servidor de arquivos ver o pedido.
func somenteLeitura(prox http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodGet && r.Method != http.MethodHead {
			http.Error(w, "método não permitido", http.StatusMethodNotAllowed)
			return
		}
		prox.ServeHTTP(w, r)
	})
}

type randomico struct{}

func (randomico) Read(p []byte) (int, error) { return leituraAleatoria(p) }

func main() {
	cfg := carregar()
	if cfg.Token == "" {
		log.Println("aviso: sem token do GitHub; todo relato vai para a fila")
	}
	if err := (Fila{Diretorio: cfg.Fila}).Conferir(); err != nil {
		// Não é motivo para não subir: servir o repositório continua
		// funcionando, e é metade do serviço. Mas precisa ser dito alto.
		log.Printf("AVISO: não consigo escrever em %s (%v). "+
			"Relatos serão PERDIDOS se o GitHub recusar. "+
			"Monte um volume gravável ali.", cfg.Fila, err)
	}

	s := &Servico{
		cfg:       cfg,
		balde:     NovoBalde(cfg.PorHora, cfg.Teto),
		repetidos: NovosRepetidos(6 * time.Hour),
		github: GitHub{
			Base:        ambiente("ADVBR_GITHUB_BASE", "https://api.github.com"),
			Repositorio: cfg.RepoIssues,
			Token:       cfg.Token,
		},
		fila:         Fila{Diretorio: cfg.Fila},
		verificacoes: make(chan struct{}, cfg.Simultaneas),
	}

	// Duas tarefas de fundo, ambas baratas: reenviar o que ficou na fila e
	// esquecer quem não aparece há um dia.
	go func() {
		for {
			time.Sleep(10 * time.Minute)
			s.fila.Reenviar(s.github, log.Printf)
			s.balde.Esquecer(24 * time.Hour)
		}
	}()

	servidor := &http.Server{
		Addr:              cfg.Endereco,
		Handler:           s.rotas(),
		ReadHeaderTimeout: 10 * time.Second,
		ReadTimeout:       30 * time.Second,
		WriteTimeout:      60 * time.Second,
		IdleTimeout:       2 * time.Minute,
	}
	fmt.Printf("adv-br: escutando em %s, servindo %s\n", cfg.Endereco, cfg.Repositorio)
	log.Fatal(servidor.ListenAndServe())
}
