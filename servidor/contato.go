package main

import (
	"errors"
	"regexp"
	"strings"
)

// O e-mail de contato que a pessoa informa no relato, para quem mantém o
// aplicativo pedir mais detalhes e avisar quando estiver resolvido.
//
// É o único campo do relato que NÃO passa pela sanitização: ele existe para
// chegar inteiro, e a pessoa o escreveu sabendo para quê. Por isso a forma é
// fechada. Sem isto, o campo seria a porta para pôr na issue o que a
// sanitização tira do resto, ou Markdown e menções do GitHub. A regra é a
// mesma de ui/relator.py, e os casos estão em tests/casos-contato.json.
var formaDoContato = regexp.MustCompile(
	`^[A-Za-z0-9._%+-]{1,64}@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}$`)

// TamanhoMaximoContato é o limite de um endereço de e-mail (RFC 5321).
const TamanhoMaximoContato = 254

var ErrContatoInvalido = errors.New("o e-mail de contato não parece um endereço de e-mail")

// ValidarContato devolve o contato sem espaços nas pontas, ou "" se a pessoa
// não informou nenhum. Um contato que não tenha forma de e-mail é recusado, e
// não descartado: quem o escreveu espera ser procurado.
func ValidarContato(contato string) (string, error) {
	contato = strings.TrimSpace(contato)
	if contato == "" {
		return "", nil
	}
	if len(contato) > TamanhoMaximoContato || !formaDoContato.MatchString(contato) {
		return "", ErrContatoInvalido
	}
	return contato, nil
}
