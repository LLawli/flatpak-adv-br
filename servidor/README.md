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
aparece em `podman inspect`, no `ps` e em qualquer despejo de ambiente.

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

```sh
podman run -d --name adv-br --restart=always \
    --network host --memory 128m --read-only --cap-drop=ALL \
    -e GOMEMLIMIT=64MiB \
    -e ADVBR_ENDERECO=127.0.0.1:8080 \
    -e ADVBR_REPOSITORIO=/repo \
    -e ADVBR_TOKEN_GITHUB_FILE=/run/secrets/github \
    -v /srv/adv-br/repo:/repo:ro,Z \
    -v /srv/adv-br/fila:/var/lib/adv-br/fila:Z,U \
    -v /srv/adv-br/token:/run/secrets/github:ro,Z \
    adv-br-servico:producao
```

`--read-only` e `--cap-drop=ALL` valem: a imagem não tem shell nem gerenciador
de pacotes, e o processo não precisa escrever em lugar nenhum além da fila.

O `,U` no volume da fila não é enfeite. Com podman sem privilégio, o usuário
10001 de dentro do container mapeia para um subuid do host, que não é dono do
diretório: sem ele a fila não é gravável e todo relato que o GitHub recusar se
perde. O serviço avisa disso na subida, em vez de deixar a descoberta para o
primeiro relato perdido:

    AVISO: não consigo escrever em /var/lib/adv-br/fila (read-only file system).
    Relatos serão PERDIDOS se o GitHub recusar. Monte um volume gravável ali.

## Caddy na frente

```caddy
flatpak.lukakuuhaku.dev {
    reverse_proxy 127.0.0.1:8080
}
```

Só isso. O Caddy cuida do certificado sozinho e já manda o `X-Forwarded-For`,
que é o cabeçalho de que o limite por endereço depende: sem ele, todo mundo
conta como um endereço só, o do proxy. (Com nginx seria preciso acrescentar
`proxy_set_header X-Forwarded-For $remote_addr;` à mão.)

Se quiser cache dos objetos do repositório, eles nunca mudam de conteúdo: o
nome de cada um é o hash do que ele contém. O que muda é o `summary`, e esse
não deve ser cacheado por muito tempo, senão uma versão nova demora a aparecer
para quem já tem o remoto configurado.

## Publicar uma versão

O repositório é construído e assinado na máquina de quem publica; a chave
privada nunca chega aqui. O que sobe é o resultado:

```sh
rsync -a --delete ~/.local/share/adv-br-repo/ vps:/srv/adv-br/repo/
```

Só os objetos novos atravessam. Não é preciso reiniciar o serviço: ele lê do
disco a cada pedido.
