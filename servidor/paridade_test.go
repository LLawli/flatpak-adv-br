package main

import (
	"encoding/json"
	"os"
	"strings"
	"testing"
)

// Os mesmos casos que o aplicativo usa, em tests/casos-sanitizacao.json.
//
// A limpeza do aplicativo é a que a pessoa vê antes de enviar; a daqui é a que
// protege contra uma versão antiga dele. Se as duas divergirem, ou vaza dado
// pessoal ou o relato chega inútil, e nos dois casos ninguém percebe até ser
// tarde. Por isso o arquivo de casos é um só.
func TestParidadeComOAplicativo(t *testing.T) {
	bruto, err := os.ReadFile("../tests/casos-sanitizacao.json")
	if err != nil {
		t.Skipf("sem o arquivo de casos: %v", err)
	}

	var arquivo struct {
		Casos []struct {
			Nome    string   `json:"nome"`
			Entrada string   `json:"entrada"`
			Some    []string `json:"some"`
			Fica    []string `json:"fica"`
		} `json:"casos"`
	}
	if err := json.Unmarshal(bruto, &arquivo); err != nil {
		t.Fatalf("casos ilegíveis: %v", err)
	}
	if len(arquivo.Casos) < 5 {
		t.Fatalf("esperava ao menos 5 casos, achei %d", len(arquivo.Casos))
	}

	for _, caso := range arquivo.Casos {
		saida := Sanitizar(caso.Entrada)
		for _, proibido := range caso.Some {
			if strings.Contains(saida, proibido) {
				t.Errorf("%s: %q sobreviveu em %q", caso.Nome, proibido, saida)
			}
		}
		for _, necessario := range caso.Fica {
			if !strings.Contains(saida, necessario) {
				t.Errorf("%s: a limpeza comeu %q de %q", caso.Nome, necessario, saida)
			}
		}
	}
}
