"""Tira o dado pessoal de um texto, antes de ele sair desta máquina.

Isto roda no aplicativo, e o resultado é o que a pessoa vê no diálogo antes de
enviar: ela confere o texto já limpo, e não uma promessa de que será limpo
depois. O serviço aplica as mesmas regras de novo, porque a versão do
aplicativo que enviou não é algo que ele controle.

O que se protege, concretamente: o rótulo de um token ICP-Brasil é o nome do
titular seguido do CPF, e ele aparece em toda listagem de certificado. Os
caminhos dos logs carregam o nome de usuário do sistema. Nada disso pode acabar
num repositório, nem privado.

As regras estão em tests/casos-sanitizacao.json, lidas também pelo teste do
serviço em Go: se os dois lados divergirem, ou vaza dado pessoal ou o relato
chega inútil.
"""
import re

# A ordem importa: o rótulo inteiro vem antes da regra de CPF, senão sobra o
# nome sozinho, que continua identificando a pessoa.
LIMPEZAS = [
    ("titular", re.compile(r"[A-ZÁÀÂÃÉÊÍÓÔÕÚÜÇ][A-ZÁÀÂÃÉÊÍÓÔÕÚÜÇ\s.'-]{4,}:\d{11}"),
     "[TITULAR]"),
    ("cpf", re.compile(r"\b\d{3}\.?\d{3}\.?\d{3}-?\d{2}\b"), "[CPF]"),
    ("cnpj", re.compile(r"\b\d{2}\.?\d{3}\.?\d{3}/?\d{4}-?\d{2}\b"), "[CNPJ]"),
    ("email", re.compile(r"\b[\w.%+-]+@[\w.-]+\.[A-Za-z]{2,}\b"), "[EMAIL]"),
    # O home aparece em todo caminho de log. /var/home é o dos sistemas
    # atômicos, como o Fedora Silverblue.
    ("home", re.compile(r"/(?:var/)?home/[^/\s:\"']+"), "~"),
    # Serial e impressão digital identificam o titular tão bem quanto o nome,
    # para quem tem a lista.
    ("serial", re.compile(r"\b(?:[0-9a-fA-F]{2}:){7,}[0-9a-fA-F]{2}\b"), "[SERIAL]"),
    ("impressao", re.compile(r"\b[0-9a-fA-F]{40,64}\b"), "[IMPRESSAO]"),
]


def sanitizar(texto):
    """Devolve o texto sem os dados pessoais que sabemos reconhecer."""
    for _, achar, por in LIMPEZAS:
        texto = achar.sub(por, texto)
    return texto
