# Imagem do serviço: um binário estático sobre nada.
#
# O resultado não tem shell, gerenciador de pacotes nem biblioteca do sistema.
# Não é purismo: um serviço exposto à internet que guarda um token do GitHub é
# exatamente onde não se quer um shell disponível para quem encontrar uma
# falha. O preço é depurar de fora, com os logs.
FROM docker.io/library/golang:alpine AS construcao

WORKDIR /fonte
# As dependências primeiro: assim uma mudança no código não rebaixa o cache.
COPY go.mod go.sum ./
RUN go mod download
COPY *.go ./

# CGO desligado é o que torna o binário independente da libc da imagem base.
# O -trimpath tira os caminhos da máquina de quem construiu de dentro do
# binário, e o -s -w tira a tabela de símbolos: menos coisa para vazar, menos
# MB para enviar por ssh.
RUN CGO_ENABLED=0 go build -trimpath -ldflags="-s -w" -o /servidor .

FROM scratch

# Os certificados de raiz, para o cliente HTTPS conseguir falar com o GitHub.
# Sem isto o serviço sobe, atende, e falha em toda publicação com "certificate
# signed by unknown authority", que parece problema de token.
COPY --from=construcao /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/
COPY --from=construcao /servidor /servidor

# Sem usuário nomeado numa imagem sem /etc/passwd: o número basta, e é o que o
# kernel usa. 10001 está fora da faixa de usuários de sistema.
USER 10001:10001

EXPOSE 8080
ENTRYPOINT ["/servidor"]
