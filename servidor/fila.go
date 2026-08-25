package main

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"time"
)

// A fila de relatos que o GitHub recusou na hora.
//
// É a única coisa que este serviço escreve em disco, e existe por um motivo
// só: quem relatou um problema não deve perder o relato porque o GitHub estava
// fora do ar, expirou um token ou passou de um limite de API. A pessoa já fez
// a parte dela.
//
// Um diretório com arquivos JSON, e não um banco: são poucos, ficam pouco
// tempo, e um `ls` diz o que está pendente.
type Fila struct {
	Diretorio string
}

// Conferir diz se dá para escrever na fila. Serve para reclamar na subida, e
// não no primeiro relato perdido: com --read-only e sem volume montado, a fila
// não existe, e a única hora em que isso aparecia era quando alguém já tinha
// escrito um relato e o GitHub estava fora do ar. Tarde demais.
func (f Fila) Conferir() error {
	if f.Diretorio == "" {
		return nil
	}
	if err := os.MkdirAll(f.Diretorio, 0o700); err != nil {
		return err
	}
	teste := filepath.Join(f.Diretorio, ".escrita")
	if err := os.WriteFile(teste, []byte("ok"), 0o600); err != nil {
		return err
	}
	return os.Remove(teste)
}

func (f Fila) Guardar(issue Issue) error {
	if f.Diretorio == "" {
		return nil
	}
	if err := os.MkdirAll(f.Diretorio, 0o700); err != nil {
		return err
	}
	bruto, err := json.Marshal(issue)
	if err != nil {
		return err
	}
	nome := filepath.Join(f.Diretorio,
		time.Now().UTC().Format("20060102-150405.000000000")+".json")
	return os.WriteFile(nome, bruto, 0o600)
}

// Pendentes devolve o que está guardado, do mais antigo para o mais novo.
func (f Fila) Pendentes() ([]string, error) {
	if f.Diretorio == "" {
		return nil, nil
	}
	achados, err := filepath.Glob(filepath.Join(f.Diretorio, "*.json"))
	if err != nil {
		return nil, err
	}
	sort.Strings(achados)
	return achados, nil
}

// Reenviar tenta publicar o que está na fila. O que falhar de novo continua
// lá: tentar para sempre é melhor que descartar em silêncio.
func (f Fila) Reenviar(g GitHub, registrar func(string, ...any)) {
	pendentes, err := f.Pendentes()
	if err != nil || len(pendentes) == 0 {
		return
	}
	for _, caminho := range pendentes {
		bruto, err := os.ReadFile(caminho)
		if err != nil {
			continue
		}
		var issue Issue
		if err := json.Unmarshal(bruto, &issue); err != nil {
			// Arquivo ilegível não melhora com o tempo.
			os.Rename(caminho, caminho+".invalido")
			continue
		}
		url, err := g.Publicar(issue)
		if err != nil {
			registrar("fila: %s continua pendente: %v", filepath.Base(caminho), err)
			return // se um falhou, os outros provavelmente também
		}
		os.Remove(caminho)
		registrar("fila: %s publicado em %s", filepath.Base(caminho), url)
	}
}
