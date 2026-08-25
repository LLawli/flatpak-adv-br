package main

import (
	"os/exec"
	"strings"
	"testing"
	"time"
)

func TestProvaValida(t *testing.T) {
	chave := []byte("chave de teste")
	d := EmitirDesafio(chave, 4) // dificuldade baixa, para o teste ser rápido
	nonce := Trabalhar(d.Semente, d.Dificuldade)

	if err := ValidarProva(chave, d, nonce, time.Now()); err != nil {
		t.Fatalf("prova legítima recusada: %v", err)
	}
}

func TestProvaErradaNaoPassa(t *testing.T) {
	chave := []byte("chave de teste")
	d := EmitirDesafio(chave, 8)

	if err := ValidarProva(chave, d, "0", time.Now()); err != ErrProvaFraca {
		t.Errorf("nonce qualquer devia falhar, veio %v", err)
	}
}

func TestDesafioForjadoNaoPassa(t *testing.T) {
	d := EmitirDesafio([]byte("chave do servidor"), 4)
	nonce := Trabalhar(d.Semente, d.Dificuldade)

	// Mesma semente e prova, assinada por outra chave: é o que alguém tentaria
	// ao inventar desafios em vez de pedi-los.
	if err := ValidarProva([]byte("outra chave"), d, nonce, time.Now()); err != ErrAssinatura {
		t.Errorf("desafio forjado devia ser recusado, veio %v", err)
	}
}

func TestDesafioVelhoNaoPassa(t *testing.T) {
	chave := []byte("chave de teste")
	d := EmitirDesafio(chave, 4)
	nonce := Trabalhar(d.Semente, d.Dificuldade)

	depois := time.Now().Add(ValidadeDesafio + time.Minute)
	if err := ValidarProva(chave, d, nonce, depois); err != ErrExpirado {
		t.Errorf("desafio expirado devia ser recusado, veio %v", err)
	}
}

// O cliente é Python e usa hashlib.scrypt; o servidor é Go e usa
// x/crypto/scrypt. Se as duas implementações divergirem em qualquer detalhe do
// RFC 7914, ninguém consegue relatar nada, e o erro seria "a prova não
// confere" para todo mundo. Este teste resolve o desafio EM PYTHON e valida
// aqui.
func TestCompatibilidadeComOClientePython(t *testing.T) {
	if _, err := exec.LookPath("python3"); err != nil {
		t.Skip("sem python3")
	}
	chave := []byte("chave de teste")
	d := EmitirDesafio(chave, 4)

	programa := `
import hashlib, sys
semente, dificuldade = sys.argv[1], int(sys.argv[2])
def zeros(b):
    total = 0
    for byte in b:
        if byte == 0:
            total += 8
            continue
        for deslocamento in range(7, -1, -1):
            if byte >> deslocamento & 1:
                return total
            total += 1
        return total
    return total
i = 0
while True:
    saida = hashlib.scrypt((semente + ":" + str(i)).encode(), salt=semente.encode(),
                           n=1 << 14, r=8, p=1, dklen=32, maxmem=64 * 1024 * 1024)
    if zeros(saida) >= dificuldade:
        print(i)
        break
    i += 1
`
	saida, err := exec.Command("python3", "-c", programa, d.Semente,
		"4").Output()
	if err != nil {
		t.Fatalf("o cliente em python falhou: %v", err)
	}
	nonce := strings.TrimSpace(string(saida))

	if err := ValidarProva(chave, d, nonce, time.Now()); err != nil {
		t.Fatalf("prova resolvida em Python recusada pelo Go: %v", err)
	}
}
