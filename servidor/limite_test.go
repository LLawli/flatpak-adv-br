package main

import (
	"testing"
	"time"
)

func TestBaldeSegura(t *testing.T) {
	agora := time.Now()
	b := NovoBalde(3, 3)
	b.relogio = func() time.Time { return agora }

	for i := 0; i < 3; i++ {
		if !b.Permite("1.2.3.4") {
			t.Fatalf("recusou o pedido %d, que cabia no teto", i+1)
		}
	}
	if b.Permite("1.2.3.4") {
		t.Error("deixou passar além do teto")
	}
	// Outro endereço não paga pelo primeiro.
	if !b.Permite("5.6.7.8") {
		t.Error("um endereço bloqueou o outro")
	}

	agora = agora.Add(time.Hour)
	if !b.Permite("1.2.3.4") {
		t.Error("as fichas não voltaram com o tempo")
	}
}

func TestBaldeEsquece(t *testing.T) {
	agora := time.Now()
	b := NovoBalde(1, 1)
	b.relogio = func() time.Time { return agora }
	b.Permite("1.2.3.4")

	agora = agora.Add(48 * time.Hour)
	b.Esquecer(24 * time.Hour)
	if len(b.visto) != 0 {
		t.Error("o mapa cresce para sempre")
	}
}

func TestRepetidoNaoViraDuasIssues(t *testing.T) {
	agora := time.Now()
	r := NovosRepetidos(time.Hour)
	r.relogio = func() time.Time { return agora }

	if !r.Novo("mesmo erro") {
		t.Fatal("o primeiro devia passar")
	}
	if r.Novo("mesmo erro") {
		t.Error("o repetido devia ser barrado")
	}
	if !r.Novo("outro erro") {
		t.Error("um relato diferente foi confundido com o anterior")
	}

	agora = agora.Add(2 * time.Hour)
	if !r.Novo("mesmo erro") {
		t.Error("depois da janela, o mesmo erro pode voltar")
	}
}
