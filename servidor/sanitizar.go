package main

import "regexp"

// Sanitização de dado pessoal, do lado do servidor.
//
// A mesma limpeza é feita no cliente, antes de mostrar à pessoa o que vai ser
// enviado. Esta aqui é a segunda linha, e existe porque a primeira roda numa
// versão do aplicativo que o servidor não controla: alguém com uma versão
// antiga, ou que editou o texto na caixa, não pode conseguir publicar um CPF
// numa issue.
//
// O que se protege, concretamente: o rótulo de um token ICP-Brasil é o nome do
// titular seguido do CPF, e ele aparece em toda listagem de certificado. Os
// caminhos do log carregam o nome de usuário do sistema. Isso não pode acabar
// num repositório, nem privado.
var limpezas = []struct {
	nome  string
	achar *regexp.Regexp
	por   string
}{
	// Primeiro o rótulo inteiro: NOME DA PESSOA:12345678901. Precisa vir antes
	// da regra de CPF, senão sobra o nome sozinho, que continua identificando.
	{"titular", regexp.MustCompile(`[\p{Lu}][\p{Lu}\s.'-]{4,}:\d{11}`), "[TITULAR]"},
	{"cpf", regexp.MustCompile(`\b\d{3}\.?\d{3}\.?\d{3}-?\d{2}\b`), "[CPF]"},
	{"cnpj", regexp.MustCompile(`\b\d{2}\.?\d{3}\.?\d{3}/?\d{4}-?\d{2}\b`), "[CNPJ]"},
	{"email", regexp.MustCompile(`\b[\w.%+-]+@[\w.-]+\.[A-Za-z]{2,}\b`), "[EMAIL]"},
	// O home aparece em todo caminho de log. /var/home é o dos sistemas
	// atômicos, como o Fedora Silverblue.
	{"home", regexp.MustCompile(`/(?:var/)?home/[^/\s:"']+`), "~"},
	// Serial de certificado e impressão digital: identificam o titular tão bem
	// quanto o nome, para quem tem a lista.
	{"serial", regexp.MustCompile(`\b(?i:[0-9a-f]{2}:){7,}[0-9a-f]{2}\b`), "[SERIAL]"},
	{"digest", regexp.MustCompile(`\b(?i:[0-9a-f]{40,64})\b`), "[IMPRESSAO]"},
}

// Sanitizar devolve o texto sem os dados pessoais conhecidos.
func Sanitizar(texto string) string {
	for _, limpeza := range limpezas {
		texto = limpeza.achar.ReplaceAllString(texto, limpeza.por)
	}
	return texto
}
