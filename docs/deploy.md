# Pôr o serviço no ar

Este é o roteiro do deploy, na ordem. O que cada opção significa está em
`servidor/README.md`; aqui é a sequência e o que conferir em cada passo.

O serviço faz duas coisas: serve o repositório Flatpak (arquivos que chegam por
`rsync`) e recebe relatos de erro, que viram issues num repositório privado. A
chave de assinatura do Flatpak nunca chega ao servidor: ele guarda arquivos já
assinados e não sabe assinar nada.

## O que o servidor precisa ter

- Docker, e permissão de usar sem root (grupo `docker`).
- Um proxy na frente terminando TLS. Aqui é Caddy em container, com o bouncer
  do CrowdSec compilado junto, servindo também os outros sites da máquina.
- Um nome apontado para ele. O nosso é `flatpak.lukakuuhaku.dev`, e ele vai
  dentro do `.flatpakrepo`: mudar depois obriga todo mundo a reconfigurar o
  remoto.

**Assuma que você não tem root.** Todo o deploy cabe no espaço de um usuário
comum, e é assim de propósito: um serviço exposto à internet que guarda um
token do GitHub não deveria precisar de privilégio para subir.

## 1. Estrutura

```sh
mkdir -p ~/totalidade/apps/adv-br/{repo,fila,secrets}
chmod 700 ~/totalidade/apps/adv-br/secrets
```

`repo` recebe o repositório Flatpak por `rsync` e é montado somente leitura.
`fila` guarda os relatos que o GitHub recusou na hora, e precisa ser gravável
pelo uid que roda o container. `secrets` guarda o token.

## 2. O token do GitHub

Um token com permissão de criar issues no repositório privado dos relatos, e
nada além disso:

```sh
install -m 600 /dev/null ~/totalidade/apps/adv-br/secrets/github
# escreva o token nesse arquivo com o seu editor
```

Ele entra por arquivo (`ADVBR_TOKEN_GITHUB_FILE`) e não por variável de
ambiente: o conteúdo de uma variável aparece em `docker inspect`, no `ps` e em
qualquer despejo de ambiente.

## 3. A imagem

Construída na máquina de quem desenvolve e enviada pronta. São poucos
megabytes, e o servidor não precisa baixar o SDK do Go nem gastar os núcleos
que tem:

```sh
docker build -t adv-br-servico:0.2.0 servidor/
docker save adv-br-servico:0.2.0 | ssh vps 'docker load'
```

## 4. O compose

O `compose.yaml` completo está em `servidor/README.md`, com o porquê de cada
linha. Ele vai em `~/totalidade/apps/adv-br/`, liga-se à rede externa do proxy
e não publica porta nenhuma no host: o proxy alcança o serviço pelo nome do
container.

```sh
cd ~/totalidade/apps/adv-br && docker compose up -d
docker compose logs --tail=20
```

Nos logs da subida, procure pelo aviso da fila. Se ele aparecer, o uid está
errado e todo relato recusado pelo GitHub será perdido, o que só se descobre
quando já se perdeu.

## 5. O proxy

Antes de editar, uma cópia, que é a convenção do servidor:

```sh
cd ~/totalidade/edge/proxy
cp Caddyfile Caddyfile.bak-advbr-$(date +%Y%m%d)
```

Quatro linhas no fim do arquivo:

```caddy
flatpak.lukakuuhaku.dev {
  import crowdsec_only
  import access_log
  reverse_proxy adv-br:8080
}
```

Os dois `import` são snippets que já existem, e o CrowdSec já está configurado
no bloco global: **não há credencial nenhuma a acrescentar** para pôr o serviço
no ar. Valide com o binário que está rodando, e não com a sintaxe que a memória
lembra:

```sh
docker exec proxy-caddy-1 caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile
docker exec proxy-caddy-1 caddy reload  --config /etc/caddy/Caddyfile --adapter caddyfile
```

O certificado sai sozinho por ACME, desde que o nome já resolva e a porta 80
esteja alcançável.

## 6. Publicar a primeira versão

```sh
ADV_BR_CHAVE_GPG=<id da chave> ADV_BR_VPS=vps:/home/luka/totalidade/apps/adv-br/repo \
    bin/publicar --enviar
```

Só os objetos novos atravessam: o nome de cada um é o hash do conteúdo. Não é
preciso reiniciar o serviço, que lê do disco a cada pedido.

## 7. Conferir, nesta ordem

Cada passo isola uma camada, e é por isso que a ordem importa: falhar no
segundo com o primeiro passando diz exatamente onde olhar.

| Comando | O que prova |
|---|---|
| `curl -I https://flatpak.lukakuuhaku.dev/adv-br.flatpakrepo` | proxy, TLS e roteamento |
| `curl https://flatpak.lukakuuhaku.dev/api/saude` | o serviço está de pé |
| `flatpak remote-add` e `install` numa máquina limpa | o repositório está íntegro e assinado |
| um relato de teste pelo botão do aplicativo | token, GitHub e a fila |

Vale também fazer o caminho que **precisa** falhar: publicar assinando com
outra chave e confirmar que o cliente recusa. A mensagem que ele dá nesse caso
engana, e está registrada em `docs/ui.md`.

## O que este deploy não protege

O serviço fica numa rede compartilhada com os outros containers do mesmo host,
e não numa rede dedicada só com o proxy. A consequência está no comentário de
`quemChama`, no `servidor/main.go`: o limite por endereço vale contra quem vem
da internet e não vale contra um vizinho de rede comprometido, que alcança a
porta sem passar pelo proxy e manda o `X-Forwarded-For` que quiser.

Foi decisão consciente, para não mexer no proxy que atende todos os outros
sites. Trocar de ideia é acrescentar uma rede dedicada ao compose do proxy e ao
deste serviço; nada no código muda.
