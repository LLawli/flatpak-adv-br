# Testes de ponta a ponta

Rodam no sandbox Flatpak de verdade, com extensões de teste construídas na
hora, e conferem o resultado pelo p11-kit do host. Não fazem parte do
`scripts/ci.sh`: exigem Flatpak, o SDK do GNOME 51 e uma bancada local.

## A bancada

Um diretório em disco real (o `org.flatpak.Builder` não enxerga o `/tmp` do
host), apontado por `ANCORA_BANCADA`, com:

- `libsofthsm2.so`: o SoftHSM2 (2.6.1, extraído do `.deb` do Debian trixie);
- `softhsm2.conf`: com `directories.tokendir` apontando para um diretório de
  tokens da bancada, que tenha um token inicializado chamado `HUBTESTE`.

O teste cria ali `obra/`, `home-falsa/` e `servico.log`.

## `publicacao.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/publicacao.sh
```

Instala `Driver.SoftHSM` e `Driver.Travado` (um módulo que trava ao listar
slots, `travado.c`), sobe `ancora servir` e confere: só o que responde é
publicado, o travado é parado, uma segunda instância é recusada, e SIGTERM,
SIGKILL e a partida seguinte deixam o diretório de módulos limpo. Também
desliga e religa um driver pela lista que a janela escreve, e pede a parada como
ela pede. Desinstala
as extensões de teste no fim. Leva cerca de um minuto.

## `semtoken.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/semtoken.sh
```

Instala `Driver.Vazio` (`vazio.c`, um módulo sem nenhum slot, como o driver de
uma leitora sem cartão) e confere que ele fica publicado sem token, sem ser
parado, sem sonda presa e com a lista de certificados vazia na hora.

## `aplicativos.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/aplicativos.sh
```

Exige o `org.mozilla.firefox` instalado e o `p11-kit` no host. O Âncora
acerta o Papers (permissão e banco vazio) e um perfil do Firefox (permissão e
registro do módulo) numa home falsa. Depois o NSS de dentro do Firefox Flatpak
de verdade abre esse perfil e precisa ver o token de teste exatamente uma vez,
pelo módulo do Âncora. O servidor do sistema, no meio da cadeia, é um
`p11-kit server` do host lendo a home falsa. Por fim, desfazer precisa tirar
tudo o que o Âncora pôs. `ancora configurar` liga o `p11-kit-server.socket` do
usuário no host, o que não muda nada se ele já estiver ligado.

## `nativos.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/nativos.sh
```

Publica e despublica no Firefox nativo e no `~/.pki/nssdb` de uma home falsa.
O NSS do host precisa ver o token uma vez; o mesmo `~/.pki/nssdb` aberto de
dentro de um sandbox não pode vê-lo; e a receita do Chromium Flatpak não pode
apagar a publicação nativa.

## `supervisao.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/supervisao.sh
```

Um aplicativo faz login no `Driver.Espera` (`espera.c`), que demora 25 s
segurando uma trava, como um leitor com teclado esperando o PIN, enquanto o
serviço sonda. Com a listagem de tokens disputando essa trava e sem disputa, o
login tem de terminar bem e sem ser interrompido, e o driver não pode ser
reiniciado.

## `assinadores.sh`

```bash
ANCORA_BANCADA=/mnt/dados/caches/ancora-exp tests/e2e/assinadores.sh
```

Exige o `org.mozilla.firefox` instalado. Publica um assinador de teste
(`assinador-teste.py`, que fala native messaging e carrega `libaetpkss.so.3`
pelo nome) para um Brave nativo e o Firefox Flatpak de uma home falsa, chama-o
como cada navegador chamaria e confere que ele vê o token uma vez, no sandbox
do Âncora e no do Firefox, com uma casa própria. Despublicar precisa apagar
manifestos, lançadores, links e a permissão dada.

## Isolamento

Os testes isolam a sessão com `HOME` e `XDG_CONFIG_HOME` falsos (o p11-kit acha a
configuração do usuário pelo segundo) e desligam, na home falsa, os módulos de
hardware do sistema, para não disputar a leitora com o gpg ou o ssh-agent. No
sandbox, `ANCORA_HOME_HOST` aponta o Âncora para a home falsa e `ANCORA_DADOS`
para os dados da bancada: o `XDG_DATA_HOME` não serve, porque o Flatpak o
sobrescreve e a permissão `xdg-data/ancora` traz a pasta real do host.
