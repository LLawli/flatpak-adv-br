package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// Um relato, como chega do aplicativo.
type Relato struct {
	Desafio     Desafio `json:"desafio"`
	Nonce       string  `json:"nonce"`
	Titulo      string  `json:"titulo"`
	Mensagem    string  `json:"mensagem"`
	Diagnostico string  `json:"diagnostico"`
	Versao      string  `json:"versao"`
}

// Limites de tamanho. O corpo de uma issue do GitHub para em 65536 caracteres,
// e um relato que chegue perto disso não é lido por ninguém: o que se quer é o
// fim dos logs, não o histórico inteiro.
const (
	TamanhoMaximoCorpo = 256 * 1024
	TamanhoMaximoIssue = 60000
)

// Impressao é o que identifica um relato para fins de repetição.
//
// São os campos que a pessoa escreveu e o diagnóstico, e NÃO o corpo montado:
// o corpo carrega a hora de recebimento, então dois relatos idênticos enviados
// em segundos diferentes teriam impressões diferentes e a deduplicação nunca
// pegaria nada. Foi assim na primeira versão, e o teste é que mostrou.
func Impressao(r Relato) string {
	return Sanitizar(r.Titulo) + "\x00" + Sanitizar(r.Mensagem) + "\x00" +
		Sanitizar(r.Diagnostico)
}

// Issue é o que se manda ao GitHub.
type Issue struct {
	Title  string   `json:"title"`
	Body   string   `json:"body"`
	Labels []string `json:"labels,omitempty"`
}

// Montar transforma um relato em issue, já sanitizada. Nada do que sai daqui
// pode conter dado pessoal: é o último ponto antes de o texto sair da máquina.
func Montar(r Relato, quando time.Time) Issue {
	titulo := strings.TrimSpace(Sanitizar(r.Titulo))
	if titulo == "" {
		titulo = "Relato sem título"
	}
	if len(titulo) > 120 {
		titulo = titulo[:120]
	}

	var corpo strings.Builder
	fmt.Fprintf(&corpo, "%s\n\n", strings.TrimSpace(Sanitizar(r.Mensagem)))
	fmt.Fprintf(&corpo, "Recebido em %s.\n\n", quando.UTC().Format(time.RFC3339))
	if r.Versao != "" {
		fmt.Fprintf(&corpo, "Versão: `%s`\n\n", Sanitizar(r.Versao))
	}

	diagnostico := Sanitizar(r.Diagnostico)
	if len(diagnostico) > TamanhoMaximoIssue {
		// Cortar do começo: o fim de um log é o que interessa, porque é onde
		// está o que aconteceu por último.
		diagnostico = "(início cortado)\n" + diagnostico[len(diagnostico)-TamanhoMaximoIssue:]
	}
	fmt.Fprintf(&corpo, "<details><summary>Diagnóstico</summary>\n\n```\n%s\n```\n\n</details>\n",
		diagnostico)

	return Issue{Title: titulo, Body: corpo.String(), Labels: []string{"relato"}}
}

// GitHub publica issues num repositório.
type GitHub struct {
	Base        string // trocável nos testes
	Repositorio string // dono/nome
	Token       string
	Cliente     *http.Client
}

// Publicar cria a issue. Devolve a URL, ou erro: quem chama decide se guarda
// para tentar de novo.
func (g GitHub) Publicar(issue Issue) (string, error) {
	corpo, err := json.Marshal(issue)
	if err != nil {
		return "", err
	}

	url := fmt.Sprintf("%s/repos/%s/issues", g.Base, g.Repositorio)
	pedido, err := http.NewRequest(http.MethodPost, url, bytes.NewReader(corpo))
	if err != nil {
		return "", err
	}
	pedido.Header.Set("Authorization", "Bearer "+g.Token)
	pedido.Header.Set("Accept", "application/vnd.github+json")
	pedido.Header.Set("Content-Type", "application/json")
	pedido.Header.Set("User-Agent", "adv-br-relatos")

	cliente := g.Cliente
	if cliente == nil {
		cliente = &http.Client{Timeout: 20 * time.Second}
	}
	resposta, err := cliente.Do(pedido)
	if err != nil {
		return "", err
	}
	defer resposta.Body.Close()

	lido, _ := io.ReadAll(io.LimitReader(resposta.Body, 64*1024))
	if resposta.StatusCode < 200 || resposta.StatusCode >= 300 {
		// A mensagem do GitHub entra no erro: "Bad credentials" e "Not Found"
		// (que é o que um token sem acesso ao repositório privado devolve)
		// levam a conversas diferentes.
		return "", fmt.Errorf("github respondeu %d: %s", resposta.StatusCode,
			strings.TrimSpace(string(lido)))
	}

	var criada struct {
		URL string `json:"html_url"`
	}
	json.Unmarshal(lido, &criada)
	return criada.URL, nil
}
