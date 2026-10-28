# Pôr o serviço no ar

Este é o roteiro do deploy, na ordem. O que cada opção significa está em
`servidor/README.md`; aqui é a sequência e o que conferir em cada passo.

O serviço faz duas coisas: serve o repositório Flatpak (arquivos que chegam por
`rsync`) e recebe relatos de erro, que viram issues num repositório privado. A
chave de assinatura do Flatpak nunca chega ao servidor: ele guarda arquivos já
assinados e não sabe assinar nada.

## O servidor

- **Host:** `vps.lukakuuhaku.dev`, usuário `luka` (uid 1001), alcançado pelo
  alias `vps` do `~/.ssh/config`. O acesso é só pela chave FIDO da YubiKey, um
  toque por conexão. Não existe chave comum, então **nenhum CI consegue fazer
  deploy**: todo passo daqui roda na frente da chave.
- **Um proxy na frente terminando TLS:** Caddy em container (`caddy-caddy-1`),
  com o bouncer do CrowdSec compilado junto, servindo também os outros sites da
  máquina.
- **Um nome apontado para ele:** `flatpak.lukakuuhaku.dev`. Ele vai dentro do
  `.flatpakrepo`: mudar depois obriga todo mundo a reconfigurar o remoto.

### Onde mora cada coisa

| Caminho no servidor | O que é | Quem mexe |
|---|---|---|
| `/srv/stacks/adv-br/compose.yaml` | o compose | Ansible, repositório `vps`, `stacks/adv-br/` |
| `/srv/stacks/adv-br/secrets/github` | o token do GitHub | Ansible |
| `/srv/data/adv-br/repo` | o repositório Flatpak, montado somente leitura | `bin/publicar --enviar` |
| `/srv/data/adv-br/fila` | os relatos que o GitHub recusou na hora | o container |

`/srv/stacks/` é de root e é o Ansible quem escreve lá. **Não edite nem faça
`scp` para esse diretório**: a próxima passada do playbook sobrescreve, e a
mudança some sem aviso. Tudo que é compose ou segredo muda no repositório `vps`
e se aplica com o playbook:

```sh
ansible-playbook playbooks/stacks.yml -e '{"stacks_only": ["adv-br"]}'
```

`/srv/data/` entra no backup. `repo` e `fila` são do `luka`, que é o mesmo uid
1001 com que o container roda: o `rsync` chega como dono e o serviço grava a
fila sem precisar de `chown`.

O token entra por arquivo (`ADVBR_TOKEN_GITHUB_FILE`) e não por variável de
ambiente: o conteúdo de uma variável aparece em `docker inspect`, no `ps` e em
qualquer despejo de ambiente. Ele precisa de permissão para criar issues no
repositório privado dos relatos, e nada além disso.

## 1. A imagem

Construída na máquina de quem desenvolve e enviada pronta. São poucos
megabytes, e o servidor não precisa baixar o SDK do Go nem gastar os núcleos
que tem:

```sh
docker build -t adv-br-servico:0.3.0 servidor/
docker save adv-br-servico:0.3.0 | gzip | ssh vps 'gunzip | docker load'
```

O nome carregado tem que bater exatamente com o `image:` do compose. Se não
bater, o compose tenta baixar a imagem de um registro, não acha e não sobe.

## 2. O compose

O `compose.yaml` completo, com o porquê de cada linha, está em
`servidor/README.md`; o que vale é a cópia em `stacks/adv-br/compose.yaml` do
repositório `vps`. Ele se liga à rede externa do proxy e não publica porta
nenhuma no host: o proxy alcança o serviço pelo nome do container.

Depois do playbook, confira a subida:

```sh
ssh vps 'cd /srv/stacks/adv-br && docker compose ps && docker compose logs --tail=20'
```

Nos logs, procure pelo aviso da fila. Se ele aparecer, o uid está errado e todo
relato recusado pelo GitHub será perdido, o que só se descobre quando já se
perdeu.

## 3. O proxy

O bloco do site, no Caddyfile (`/etc/caddy/Caddyfile` dentro do
`caddy-caddy-1`):

```caddy
flatpak.lukakuuhaku.dev {
  import crowdsec_only
  import access_log
  reverse_proxy adv-br:8080 {
    import segurar
  }
}
```

Os `import` são snippets que já existem, e o CrowdSec já está configurado no
bloco global: **não há credencial nenhuma a acrescentar** para pôr o serviço no
ar. Valide com o binário que está rodando, e não com a sintaxe que a memória
lembra, e recarregue sem queda:

```sh
ssh vps 'docker exec caddy-caddy-1 caddy validate --config /etc/caddy/Caddyfile --adapter caddyfile &&
    docker exec caddy-caddy-1 caddy reload --config /etc/caddy/Caddyfile'
```

O certificado sai sozinho por ACME, desde que o nome já resolva e a porta 80
esteja alcançável.

## 4. Publicar uma versão

Antes, os componentes que este projeto compila e redistribui. Eles não vêm do
repositório OSTree: são arquivos servidos em `componentes/`, e o `bin/publicar`
os leva junto de `dist/`.

```sh
bin/compilar-p11kit 0.23
bin/compilar-p11kit 0.24
bin/compilar-p11kit 0.25
bin/compilar-remoteid
```

Cada um imprime o sha256 do que gerou, e esse número precisa estar em
`ui/catalogo.py`. O `bin/publicar` confere os dois lados e **se recusa a
publicar** quando algum componente do catálogo não está em `dist/` ou está com
outro conteúdo: as duas falhas só apareceriam na máquina de quem instala, uma
como 404 e a outra como um download inteiro que o aplicativo recusa no fim.

Quem já publicou uma vez não precisa refazer isto: os arquivos continuam em
`dist/`, e o empacotamento é reprodutível (recompilar o mesmo commit dá o mesmo
sha256).

```sh
ADV_BR_CHAVE_GPG=<id da chave> ADV_BR_VPS=vps:/srv/data/adv-br/repo \
    bin/publicar --enviar
```

Só os objetos novos atravessam: o nome de cada um é o hash do conteúdo. Não é
preciso reiniciar o serviço, que lê do disco a cada pedido.

## 5. Conferir, nesta ordem

Cada passo isola uma camada, e é por isso que a ordem importa: falhar no
segundo com o primeiro passando diz exatamente onde olhar.

| Comando | O que prova |
|---|---|
| `curl -I https://flatpak.lukakuuhaku.dev/adv-br.flatpakrepo` | proxy, TLS e roteamento |
| `curl -I https://flatpak.lukakuuhaku.dev/componentes/remoteid-0.3.2.tar.gz` | os componentes que não vêm pelo OSTree chegaram |
| `curl https://flatpak.lukakuuhaku.dev/api/saude` | o serviço está de pé |
| `flatpak remote-add` e `install` numa máquina limpa | o repositório está íntegro e assinado |
| um relato de teste pelo botão do aplicativo | token, GitHub e a fila |

Vale também fazer o caminho que **precisa** falhar: publicar assinando com
outra chave e confirmar que o cliente recusa. A mensagem que ele dá nesse caso
engana, e está registrada em `docs/ui.md`.

## Atualizar o serviço

Quando o aplicativo passa a mandar um campo novo no relato, o serviço sobe
**antes** do aplicativo. O serviço antigo ignora em silêncio o campo que não
conhece, e quem atualizasse primeiro perderia o que preencheu sem aviso. Foi o
caso do e-mail de contato, na 0.3.0 do serviço e na 1.1.4 do aplicativo.

1. Construir e carregar a imagem nova, como no passo 1, com a tag nova.
2. No repositório `vps`, trocar a tag em `stacks/adv-br/compose.yaml`.
3. Rodar o playbook com `stacks_only: ["adv-br"]` e conferir os logs, como no
   passo 2.

A volta é o mesmo caminho ao contrário: devolver a tag anterior no
`compose.yaml` e rodar o playbook de novo. A imagem anterior continua carregada,
então nada precisa ser reenviado. Nos logs, de novo, procure pelo aviso da fila.

Para provar que é a versão nova sem criar issue, mande um pedido que só ela
recusa: na 0.3.0, um contato malformado volta 400 antes da prova de trabalho.

```sh
curl -s -H 'Content-Type: application/json' \
    -d '{"titulo":"x","contato":"isto não é e-mail"}' \
    https://flatpak.lukakuuhaku.dev/api/relato
```

## O que este deploy não protege

O serviço fica numa rede compartilhada com os outros containers do mesmo host
(`proxy_net`), e não numa rede dedicada só com o proxy. A consequência está no
comentário de `quemChama`, no `servidor/main.go`: o limite por endereço vale
contra quem vem da internet e não vale contra um vizinho de rede comprometido,
que alcança a porta sem passar pelo proxy e manda o `X-Forwarded-For` que
quiser.

Foi decisão consciente, para não mexer no proxy que atende todos os outros
sites. Trocar de ideia é acrescentar uma rede dedicada ao compose do proxy e ao
deste serviço, no repositório `vps`; nada no código muda.
