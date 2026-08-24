package main

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/binary"
	"errors"
	"fmt"
	"time"

	"golang.org/x/crypto/scrypt"
)

// Prova de trabalho, para o endereço de relato não virar um formulário aberto
// de criar issues.
//
// A escolha do scrypt é deliberada: ele é caro em MEMÓRIA, não só em CPU. Uma
// prova baseada só em hash é resolvida por GPU aos milhões por segundo, e não
// atrapalha quem quer abusar. Aqui cada tentativa custa 16 MB, o que também
// limita quanto se paraleliza numa máquina só.
//
// A assimetria vem da dificuldade: quem envia procura um nonce entre centenas,
// quem recebe confere um só. Medido no cliente em Python: 35 ms por tentativa,
// então 7 bits dão cerca de 4,5 segundos para resolver e 35 ms para verificar.
//
// Nada disso é pedido à pessoa: o aplicativo começa a resolver quando o
// diálogo abre e termina enquanto ela escreve o que aconteceu.
const (
	scryptN = 1 << 14 // 16 MB por tentativa
	scryptR = 8
	scryptP = 1

	// Quantos bits zero à frente do resultado. Cada bit dobra o custo de
	// resolver e não muda o de verificar.
	DificuldadePadrao = 7

	// Uma semente velha não vale: sem isto, alguém resolveria uma vez e
	// reenviaria para sempre.
	ValidadeDesafio = 5 * time.Minute
)

// Desafio é o que o cliente recebe para começar a trabalhar.
type Desafio struct {
	Semente     string `json:"semente"`
	Assinatura  string `json:"assinatura"`
	Dificuldade int    `json:"dificuldade"`
	N           int    `json:"n"`
	R           int    `json:"r"`
	P           int    `json:"p"`
}

// EmitirDesafio cria um desafio novo. O servidor NÃO guarda nada: a validade e
// a autenticidade vão dentro da própria semente, e o HMAC é o que impede
// alguém de forjar uma.
func EmitirDesafio(chave []byte, dificuldade int) Desafio {
	bruto := make([]byte, 8+16)
	binary.BigEndian.PutUint64(bruto[:8], uint64(time.Now().Unix()))
	rand.Read(bruto[8:])

	semente := base64.RawURLEncoding.EncodeToString(bruto)
	return Desafio{
		Semente:     semente,
		Assinatura:  assinar(chave, semente),
		Dificuldade: dificuldade,
		N:           scryptN,
		R:           scryptR,
		P:           scryptP,
	}
}

func assinar(chave []byte, semente string) string {
	mac := hmac.New(sha256.New, chave)
	mac.Write([]byte(semente))
	return base64.RawURLEncoding.EncodeToString(mac.Sum(nil))
}

var (
	ErrAssinatura  = errors.New("desafio não foi emitido por este servidor")
	ErrExpirado    = errors.New("desafio expirado; peça outro")
	ErrProvaFraca  = errors.New("a prova de trabalho não confere")
	ErrSementeRuim = errors.New("semente malformada")
)

// ValidarProva confere que o desafio é nosso, está no prazo, e que o nonce
// resolve. É o caminho caro do servidor: uma passada de scrypt, 16 MB.
func ValidarProva(chave []byte, d Desafio, nonce string, agora time.Time) error {
	if !hmac.Equal([]byte(assinar(chave, d.Semente)), []byte(d.Assinatura)) {
		return ErrAssinatura
	}

	bruto, err := base64.RawURLEncoding.DecodeString(d.Semente)
	if err != nil || len(bruto) < 8 {
		return ErrSementeRuim
	}
	emitido := time.Unix(int64(binary.BigEndian.Uint64(bruto[:8])), 0)
	if agora.Sub(emitido) > ValidadeDesafio || emitido.After(agora.Add(time.Minute)) {
		return ErrExpirado
	}

	if !Resolve(d.Semente, nonce, d.Dificuldade) {
		return ErrProvaFraca
	}
	return nil
}

// Resolve diz se aquele nonce satisfaz o desafio. É a mesma conta que o cliente
// faz, e precisa continuar sendo: os parâmetros estão no desafio justamente
// para que os dois lados não possam divergir em silêncio.
func Resolve(semente, nonce string, dificuldade int) bool {
	saida, err := scrypt.Key([]byte(semente+":"+nonce), []byte(semente),
		scryptN, scryptR, scryptP, 32)
	if err != nil {
		return false
	}
	return zerosIniciais(saida) >= dificuldade
}

func zerosIniciais(b []byte) int {
	total := 0
	for _, byte_ := range b {
		if byte_ == 0 {
			total += 8
			continue
		}
		for mascara := byte(0x80); mascara > 0; mascara >>= 1 {
			if byte_&mascara != 0 {
				return total
			}
			total++
		}
		return total
	}
	return total
}

// Trabalhar procura um nonce. Existe para os testes e para a ferramenta de
// linha de comando; em produção quem faz isto é o cliente.
func Trabalhar(semente string, dificuldade int) string {
	for i := 0; ; i++ {
		nonce := fmt.Sprintf("%d", i)
		if Resolve(semente, nonce, dificuldade) {
			return nonce
		}
	}
}
