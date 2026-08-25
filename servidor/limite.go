package main

import (
	"crypto/sha256"
	"encoding/hex"
	"sync"
	"time"
)

// Limites, sem banco de dados.
//
// Tudo aqui vive em memória e some num reinício, e está certo assim: o que se
// protege é o serviço contra enxurrada, não um saldo que precise sobreviver.
// Um atacante que reinicie o processo para zerar o contador já teria de
// derrubar o serviço, que é problema maior.

// Balde é um token bucket por chave (o endereço de quem chama).
type Balde struct {
	mu      sync.Mutex
	fichas  map[string]float64
	visto   map[string]time.Time
	porHora float64
	teto    float64
	relogio func() time.Time
}

func NovoBalde(porHora, teto float64) *Balde {
	return &Balde{
		fichas:  map[string]float64{},
		visto:   map[string]time.Time{},
		porHora: porHora,
		teto:    teto,
		relogio: time.Now,
	}
}

// Permite desconta uma ficha, se houver. As fichas voltam com o tempo.
func (b *Balde) Permite(chave string) bool {
	b.mu.Lock()
	defer b.mu.Unlock()

	agora := b.relogio()
	anterior, existia := b.visto[chave]
	saldo := b.teto
	if existia {
		saldo = b.fichas[chave] + agora.Sub(anterior).Hours()*b.porHora
		if saldo > b.teto {
			saldo = b.teto
		}
	}
	b.visto[chave] = agora

	if saldo < 1 {
		b.fichas[chave] = saldo
		return false
	}
	b.fichas[chave] = saldo - 1
	return true
}

// Esquecer joga fora quem não aparece há um tempo, para o mapa não crescer sem
// limite num serviço que fica meses de pé.
func (b *Balde) Esquecer(idade time.Duration) {
	b.mu.Lock()
	defer b.mu.Unlock()

	limite := b.relogio().Add(-idade)
	for chave, quando := range b.visto {
		if quando.Before(limite) {
			delete(b.visto, chave)
			delete(b.fichas, chave)
		}
	}
}

// Repetidos guarda as impressões dos relatos recentes, para o mesmo erro
// enviado três vezes não virar três issues. É uma janela, não um histórico: o
// que sai dela pode voltar, e tudo bem.
type Repetidos struct {
	mu      sync.Mutex
	quando  map[string]time.Time
	janela  time.Duration
	relogio func() time.Time
}

func NovosRepetidos(janela time.Duration) *Repetidos {
	return &Repetidos{quando: map[string]time.Time{}, janela: janela, relogio: time.Now}
}

// Novo diz se é a primeira vez que este conteúdo aparece na janela.
func (r *Repetidos) Novo(conteudo string) bool {
	soma := sha256.Sum256([]byte(conteudo))
	impressao := hex.EncodeToString(soma[:])

	r.mu.Lock()
	defer r.mu.Unlock()

	agora := r.relogio()
	for chave, quando := range r.quando {
		if agora.Sub(quando) > r.janela {
			delete(r.quando, chave)
		}
	}
	if _, repetido := r.quando[impressao]; repetido {
		return false
	}
	r.quando[impressao] = agora
	return true
}
