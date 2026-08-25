# O serviço

Serve o repositório Flatpak e recebe os relatos de erro do aplicativo,
transformando cada um numa issue do GitHub. Um binário, sem banco de dados.

## Por que existe

Para criar uma issue é preciso um token, e um token embutido num aplicativo
distribuído não é um segredo: qualquer pessoa o extrai do pacote. Aqui ele fica
no servidor, e o que o aplicativo manda é um texto já sanitizado com uma prova
de trabalho.

Servir o repositório no mesmo processo é decisão de operação: um container, um
volume, um lugar para olhar.

## Configuração

Tudo por variável de ambiente. Qualquer uma aceita o sufixo `_FILE` apontando
para um arquivo, e é assim que o token deve entrar: o conteúdo de uma variável
aparece em `docker inspect`, no `ps` e em qualquer despejo de ambiente.

| Variável | Padrão | O que é |
|---|---|---|
| `ADVBR_ENDERECO` | `127.0.0.1:8080` | onde escutar |
| `ADVBR_REPOSITORIO` | `/var/lib/adv-br/repo` | diretório servido como arquivos |
| `ADVBR_REPO_ISSUES` | `LLawli/adv-br-relatos` | dono/nome no GitHub |
| `ADVBR_TOKEN_GITHUB` | (vazio) | token com permissão de criar issue |
| `ADVBR_FILA` | `/var/lib/adv-br/fila` | onde guardar o que o GitHub recusou |
| `ADVBR_DIFICULDADE` | `7` | bits da prova de trabalho |
| `ADVBR_RELATOS_POR_HORA` | `5` | por endereço |
| `ADVBR_VERIFICACOES_SIMULTANEAS` | `2` | teto de provas verificadas ao mesmo tempo |
| `ADVBR_CHAVE_DESAFIO` | (aleatória) | fixe para os desafios sobreviverem a um reinício |

Sem token, o serviço sobe e guarda tudo na fila. É o modo de teste.

## Quanto de memória reservar

Medido, com o pior caso que existe: 16 pedidos simultâneos com prova inválida,
que é o mais barato de mandar e custa ao servidor o mesmo que uma prova boa.

| Situação | Memória |
|---|---|
| Em repouso | 2 a 3 MB |
| Servindo arquivos | 15 a 20 MB |
| 16 pedidos simultâneos, sem `GOMEMLIMIT` | 75 MB |
| 16 pedidos simultâneos, com `GOMEMLIMIT=64MiB` | 58 MB |

**Reserve 128 MB e passe `GOMEMLIMIT=64MiB`.** Com 96 MB também funciona (foi
testado, com pico de 58 MB), mas sem folga para um pico maior.

O que sustenta esses números é o teto de verificações simultâneas. Antes de ele
existir, oito pedidos paralelos passavam de 128 MB e o container era morto pelo
OOM killer: cada verificação de prova aloca os mesmos 16 MB que o cliente gastou
para resolver. Se aumentar `ADVBR_VERIFICACOES_SIMULTANEAS`, some 16 MB por
unidade ao limite.

## Rodar

A VPS usa Docker, e o padrão dela é um `compose.yaml` por serviço, ligado à
rede externa `proxy_net`, sem publicar porta nenhuma no host: o Caddy alcança
cada serviço pelo nome do container. Este segue o mesmo padrão, em
`~/totalidade/apps/adv-br/compose.yaml`:

```yaml
services:
  adv-br:
    image: adv-br-servico:0.2.0
    user: "1001:1001"
    networks: [proxy_net]
    expose: ["8080"]
    restart: unless-stopped
    read_only: true
    cap_drop: [ALL]
    security_opt: ["no-new-privileges:true"]
    mem_limit: 128m
    environment:
      GOMEMLIMIT: 64MiB
      ADVBR_ENDERECO: "0.0.0.0:8080"
      ADVBR_REPOSITORIO: /repo
      ADVBR_TOKEN_GITHUB_FILE: /run/secrets/github
    volumes:
      - ./repo:/repo:ro
      - ./fila:/var/lib/adv-br/fila
      - ./secrets/github:/run/secrets/github:ro

networks:
  proxy_net:
    external: true
```

Três linhas aí merecem explicação, porque cada uma custou uma descoberta:

**`ADVBR_ENDERECO: "0.0.0.0:8080"`.** O padrão do binário é `127.0.0.1:8080`,
que é o certo para desenvolvimento e o errado dentro de um container: o
localhost de lá é só dele, e o proxy bate numa porta fechada. O sintoma é um
502 do Caddy com o serviço subindo normalmente nos logs.

**`user: "1001:1001"`.** O Dockerfile roda como 10001, um uid sem dono no host.
A fila precisa ser gravável, e `chown` para outro uid pede root, que nesta VPS
pede senha. Rodar com o uid de quem é dono do diretório resolve sem privilégio
e sem afrouxar permissão. O serviço avisa na subida se errar isso:

    AVISO: não consigo escrever em /var/lib/adv-br/fila (read-only file system).
    Relatos serão PERDIDOS se o GitHub recusar. Monte um volume gravável ali.

**`read_only` e `cap_drop: [ALL]`.** A imagem não tem shell nem gerenciador de
pacotes, e o processo não escreve em lugar nenhum além da fila.

A imagem é construída na máquina de quem desenvolve e vai pronta, porque a VPS
tem dois núcleos e nada a ganhar baixando o SDK do Go:

```sh
docker build -t adv-br-servico:0.2.0 servidor/
docker save adv-br-servico:0.2.0 | ssh vps 'docker load'
```

O token do GitHub fica em `secrets/github`, com permissão 600, e entra por
arquivo e não por variável de ambiente. Veja "Configuração".

## Caddy na frente

O proxy da VPS é um Caddy em container (`proxy-caddy-1`), com o Caddyfile em
`~/totalidade/edge/proxy/Caddyfile`. Um site novo são quatro linhas, no padrão
que os outros já usam:

```caddy
flatpak.lukakuuhaku.dev {
  import crowdsec_only
  import access_log
  reverse_proxy adv-br:8080
}
```

`crowdsec_only` e `access_log` são snippets que já existem lá; o bloco global
já configura o CrowdSec, então não há credencial nenhuma a acrescentar. O
certificado sai sozinho por ACME.

Validar antes de recarregar, com o binário que roda lá e não com a sintaxe que
a memória lembra:

```sh
docker exec proxy-caddy-1 caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
docker exec proxy-caddy-1 caddy reload  --config /etc/caddy/Caddyfile --adapter caddyfile
```

O Caddy manda o `X-Forwarded-For` por conta própria em todo `reverse_proxy`,
que é o cabeçalho de que o limite por endereço depende: sem ele, todo mundo
conta como um endereço só, o do proxy. (Com nginx seria preciso acrescentar
`proxy_set_header X-Forwarded-For $remote_addr;` à mão.)

O que esse cabeçalho não protege está em `quemChama`, no `main.go`, e vale
repetir aqui: na rede `proxy_net` moram outros serviços, e qualquer um deles
alcança a porta 8080 sem passar pelo proxy. O limite por endereço vale contra a
internet, não contra um vizinho comprometido.

Se quiser cache dos objetos do repositório, eles nunca mudam de conteúdo: o
nome de cada um é o hash do que ele contém. O que muda é o `summary`, e esse
não deve ser cacheado por muito tempo, senão uma versão nova demora a aparecer
para quem já tem o remoto configurado.

## Publicar uma versão

O repositório é construído e assinado na máquina de quem publica; a chave
privada nunca chega aqui. O que sobe é o resultado:

```sh
rsync -a --delete ~/.local/share/adv-br-repo/ vps:/home/luka/totalidade/apps/adv-br/repo/
```

Só os objetos novos atravessam. Não é preciso reiniciar o serviço: ele lê do
disco a cada pedido.
