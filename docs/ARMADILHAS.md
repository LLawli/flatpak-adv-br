# Armadilhas, medidas

Tudo aqui foi observado executando, não deduzido. Cada seção diz o que se viu,
por que enganou e o que resolveu.

## O Flatpak fecha a porta da configuração do p11-kit, e abre outra

Todo sandbox Flatpak recebe um `/etc/pkcs11/pkcs11.conf` com
`user-config: none`. A string está no binário do `flatpak`, não no runtime,
então vale para qualquer app, runtime e distribuição. Nenhum `.module` de
usuário é lido lá dentro.

A porta que fica aberta é o `p11-kit-client.so` do runtime: ele é carregado
**pelo banco NSS do perfil**, não pela configuração do p11-kit, e fala com um
socket. Ligando o `p11-kit-server.socket` do host e dando ao app
`--filesystem=xdg-run/p11-kit/pkcs11`, o sandbox passa a enxergar tudo o que o
p11-kit do host conhece.

**O que engana:** `/run/user/$UID/p11-kit` já existe dentro do sandbox antes de
qualquer configuração, com um socket do próprio Flatpak. Dá para olhar e
concluir "já atravessa". Compare o inode e o mtime, não a existência.

## O `p11-kit-trust.module` do sandbox não sai

O Flatpak escreve `/etc/pkcs11/modules/p11-kit-trust.module` em todo sandbox.
`/etc` é tmpfs gravável, mas esse arquivo é um **bind somente-leitura**: `rm`
devolve "Dispositivo ou recurso está ocupado" e sobrescrever devolve "Sistema de
arquivos somente para leitura". O `/etc/pkcs11/pkcs11.conf` também.

Consequência de projeto: quem exportar o `p11-kit-proxy` do sandbox devolve
junto os tokens "System Trust" e "Default Trust". Por isso a ponte deste projeto
exporta **um módulo por processo**, e não o proxy.

## O Fedora não empacota `p11-kit-client.so`

O pacote `p11-kit` do Fedora 44 entrega `p11-kit-proxy.so` e
`/usr/libexec/p11-kit/p11-kit-remote`, e não o `client.so`. Receita que mande o
host carregar `p11-kit-client.so` falha ali com "No such file or directory". No
host o consumo é pelo **proxy**; o `client.so` é peça do lado de dentro do
sandbox, e existe nos runtimes.

## Um banco NSS na home é lido pelos dois lados

Um app Flatpak com `--filesystem=home` (o Papers tem `home:ro`) **não** usa o
`~/.var/app/<id>` como home: `$HOME` dentro dele é a home real. O banco NSS que
ele abre é o seu, `~/.pki/nssdb`.

Como as bibliotecas ficam em caminhos diferentes nos dois mundos, o
`pkcs11.txt` recebe os dois registros: o `p11-kit-proxy.so` do host e o
`p11-kit-client.so` do runtime. O NSS ignora em silêncio o que não conseguir
carregar, e em cada lado é justamente um deles que não carrega.

Conferir de que tipo é o app: `flatpak info --show-permissions <app>`.

## `modutil` é dispensável, e `certutil` também

Num banco moderno (cert9.db + key4.db) a lista de módulos não vive dentro do
banco: vive em `pkcs11.txt`, texto puro ao lado dele. É esse arquivo que o
`modutil -add` edita. Editá-lo direto evita exigir `nss-tools`, que num Fedora
atômico custa um `rpm-ostree` e um reboot. E resolve de graça o caso que
exigiria `modutil -rawadd`: registrar, do host, um caminho que só existe dentro
de um sandbox.

Criar um banco vazio dispensa o `certutil`:

```python
ctypes.CDLL("libnss3.so").NSS_Initialize(b"sql:/caminho", b"", b"", b"secmod.db", 0)
```

## `fallback-x11` mata o Lacuna e o Softplan

Os dois assinadores são .NET com Avalonia, que só tem backend X11. Sob Wayland
eles morrem antes de ler a primeira mensagem:

```
Unhandled exception. System.Exception: XOpenDisplay failed
   at Avalonia.X11.AvaloniaX11Platform.Initialize
```

`--socket=fallback-x11` só entrega o socket X11 quando **não** há Wayland, que é
exatamente o caso em que eles não precisariam dele. Tem de ser `--socket=x11`.

**O que engana:** visto do teste, o sintoma é "o assinador não respondeu", que
se confunde com ponte quebrada.

## O Certisign WebSigner não responde a teste automatizado

`cswebsigner` sai com código 1, sem stdout e sem stderr, para `getVersion`,
`getInfo` e `listTokens`, com e sem a origem que o manifesto dele autoriza. Ele
faz o mesmo **no host, fora de qualquer sandbox, e dentro de um contêiner
Debian**, então não é empacotamento. Os comandos que o binário conhece são
`getInfo`, `listCertificates`, `listTokens`, `sign` e `signCades`; `getVersion`
não está lá. Quem o exercita de verdade é a extensão no navegador.

Ele também carrega driver por `dlopen` de nome simples (`libeToken.so`,
`libeTPkcs11.so`), o que o `LD_LIBRARY_PATH` das extensões já resolve.

## A raiz do sandbox é gravável, e `/usr` não

O `cswebsigner` abre `/opt/certisign-websigner/res/*.glade` por caminho
absoluto. `/usr` pertence ao runtime e é somente-leitura, mas `/` é um tmpfs
gravável: `mkdir -p /opt/certisign-websigner` funciona, e um symlink para
`/app/opt/...` resolve sem remendar string dentro do binário.

É o mesmo mecanismo que dá o `/pkcs11/adv-br.so` que se digita na extensão.

## A Softplan serve uma cadeia TLS incompleta

`websigner.softplan.com.br` repete o certificado do servidor no lugar do
intermediário, e qualquer cliente que valide TLS corretamente recusa o
download. A saída comum é `curl -k`, que desliga a verificação inteira.

Aqui a cadeia é **completada**: `packaging/ca/ThawteTLSRSACAG1.pem` é o
intermediário que falta, emitido pela DigiCert Global Root G2, que o sistema já
confia. Com ele, `openssl s_client` devolve `Verify return code: 0 (ok)`.

O downloader do `flatpak-builder` **ignora `SSL_CERT_FILE` e `CURL_CA_BUNDLE`**
(testado). Por isso esse é o único módulo que baixa dentro do build
(`build-args: --share=network`), com `curl --cacert` e `sha256sum -c` em
seguida.

## Um driver de token que você não usa custa um minuto por abertura

Com a extensão **SafeNet** instalada e **sem** token SafeNet espetado,
`C_Initialize` + `C_GetSlotList` levam mais de 60 s dentro do sandbox e devolvem
zero slots. É esse tempo que o navegador espera ao abrir, porque ele enumera os
slots de todos os módulos registrados.

O que foi isolado, medindo:

| condição | tempo |
|---|---|
| a mesma biblioteca no host, fora de qualquer sandbox | 1,9 s (e SIGSEGV ao sair) |
| no sandbox, com `--socket=pcsc` | > 60 s |
| no sandbox, **sem** `--socket=pcsc` | 0,2 s |
| no sandbox, sem `--device=all`, ou sem `--share=network`, ou sem os `/etc/eToken*.conf` | > 60 s |
| a mesma biblioteca dentro de **outro** Flatpak (`br.jus.cnj.PJeOffice`) | > 60 s |

Ou seja: é a conversa com o pcscd através do socket montado no sandbox, e não
é o empacotamento deste projeto: acontece igual em outro pacote. O processo
fica em `clock_nanosleep`, o que tem cara de laço de nova tentativa dentro da
`libeToken`.

**O que fazer:** instale só o driver do token que você usa. A seção 6 do
`./diagnostico.sh` cronometra cada módulo publicado e avisa quando um passa de
20 s. Remover é `flatpak uninstall --user io.github.llawli.AdvBr.Driver.<Nome>`
seguido de `./host/publicar.sh`, que também apaga o `.module` órfão.

## `ldd` limpo não quer dizer que a biblioteca carrega

A `libserproidp11.so` do SerproID usa símbolos da `libgcc_s`
(`_Unwind_Resume_or_Rethrow`) e **não a declara** em `DT_NEEDED`. O `ldd` fica
verde, porque ele lista o que falta entre as dependências declaradas, e não há
nenhuma faltando. Quem abre a biblioteca com `dlopen` é que descobre:

```
undefined symbol: _Unwind_Resume_or_Rethrow
```

E descobre tarde: o p11-kit, o assinador e o PJeOffice abrem o módulo na hora
de listar certificados, que é a hora de assinar, com o token na mão. Pior: se
alguma outra coisa já tiver trazido a `libgcc_s` para o processo, funciona, e o
mesmo pacote passa numa máquina e falha em outra.

Duas consequências no projeto:

- o manifesto da extensão roda `patchelf --add-needed libgcc_s.so.1`, o que faz
  a biblioteca se bastar em qualquer processo, em vez de depender de `LD_PRELOAD`
  no chamador ou de sorte;
- as três extensões passaram a **carregar** a biblioteca no build
  (`python3 -c "import ctypes; ctypes.CDLL(...)"`), e não só a passar pelo `ldd`.

## Extensão de um app pode ser montada por outro

O Flatpak **não** exige que o prefixo de uma extensão seja o id de quem a
declara. Um pacote que escreva

```yaml
add-extensions:
  io.github.llawli.AdvBr.Driver:
    directory: lib/pkcs11/drivers
    subdirectories: true
```

recebe as extensões `io.github.llawli.AdvBr.Driver.*` instaladas na máquina,
mesmo sendo outro aplicativo. O `[ExtensionOf]` gravado na extensão diz de quem
ela é; não restringe quem a monta. Verificado montando as extensões deste
projeto dentro do `br.jus.cnj.PJeOffice`, com `p11-kit list-modules` de lá
listando os módulos.

Do lado de quem consome, `autodelete` tem de ser `false`: com `true`,
desinstalar aquele aplicativo levaria junto um driver que os outros também
usam.

## O Flatpak não exporta arquivos de extensão

O `.desktop` que uma extensão instala não vira atalho de menu. O Flatpak
exporta o que está no **aplicativo**, no momento em que ele foi construído.
Uma extensão instalada depois nunca passa por lá. Por isso o
`./host/publicar.sh` lê o `.desktop` e o ícone de dentro do sandbox e os
escreve em `~/.local/share`.

## `pkcs11Modules` vai na raiz da mensagem

Na conversa de native messaging com o Lacuna, o campo `pkcs11Modules` vai na
**raiz** do JSON. Dentro de `request` ele é ignorado em silêncio, e a resposta
vem com zero certificados.

## Provar exige chamar, não olhar

Contar módulos ou verificar se um socket existe não prova nada: o Flatpak já põe
um socket do p11-kit em todo sandbox, e um `p11-kit-proxy` sem módulo nenhum
falha igual a um driver quebrado. As provas deste repositório chamam:

- `tests/prova-pkcs11.py`: `C_GetFunctionList` → `C_Initialize` →
  `C_GetSlotList` → `C_GetTokenInfo`. O `p11-kit-proxy.so` exporta **só**
  `C_GetFunctionList`; os demais símbolos têm de sair da `CK_FUNCTION_LIST`, não
  de `dlsym`.
- `tests/prova-nss.py`: `NSS_Init` → `PK11_GetAllTokens`. O mecanismo é
  `CKM_INVALID_MECHANISM` (`0xFFFFFFFF`); passar `0` devolve só os dois slots
  internos do NSS, o que parece sucesso e não é.

## Um sandbox não enxerga `~/.var/app/<id>` inteiro

O engano natural é achar que `~/.var/app/<id>` é a home do aplicativo e que
tudo o que se põe ali ele vê. Não é. O Flatpak monta de lá apenas os
diretórios XDG (`cache`, `config`, `data`) e o que o manifesto declarar como
`persistent`. O Firefox declara `persistent=.mozilla`, e mais nada: dentro do
sandbox dele, `$HOME/.mozilla` existe e `$HOME/.local/bin` **não**.

Isso mordeu de verdade aqui. O atalho de native messaging estava sendo escrito
em `~/.var/app/<id>/.local/bin/`, o manifesto apontava para
`$HOME/.local/bin/...`, e o resultado era um arquivo que existe para o host e
não existe para o navegador. A extensão informa que o assinador não está
instalado, que é o mesmo sintoma de um manifesto no formato errado, por causa
completamente diferente.

O atalho mora agora em `~/.var/app/<id>/data/adv-br/`, que tem uma propriedade
útil: **o caminho absoluto é o mesmo dentro e fora do sandbox**. O que se grava
no manifesto vale dos dois lados, e o `./diagnostico.sh` consegue conferir, do
host, se o arquivo existe e é executável.

Conferir de qual tipo é um app: `flatpak info --show-permissions <app>`.
`filesystems=home` ou `host` significa home real; `persistent=` significa que
só aquilo é montado.

## Um componente com id próprio não consegue se registrar no barramento

O Flatpak monta um filtro de barramento de sessão mesmo sem
`--socket=session-bus`, e a política padrão dele deixa o sandbox possuir apenas
nomes que comecem pelo id do **pacote**. Um componente que chega depois e
registra um id próprio não passa.

Foi o que aconteceu com o aplicativo do RemoteID, que é `AdwApplication` com id
`dev.lukakuuhaku.RemoteID`. Ele morre no arranque com:

```
Failed to register: GDBus.Error:org.freedesktop.DBus.Error.ServiceUnknown
```

A mensagem não ajuda em nada: não fala em barramento, não fala em nome, não fala
em permissão, e `ServiceUnknown` sugere um serviço ausente em vez de um nome
recusado. Pior, ela nem parece problema de empacotamento — parece o aplicativo
não ter subido.

A correção é `--own-name`, e não `--socket=session-bus`: aquele abriria o
barramento inteiro para tudo o que roda aqui dentro, incluindo os assinadores
proprietários. Dois nomes bastam, porque o `.*` do Flatpak casa os filhos e
**não** o pai:

```yaml
  - --own-name=dev.lukakuuhaku.RemoteID
  - --own-name=dev.lukakuuhaku.RemoteID.*
```

O segundo cobre os ids que o aplicativo usa em modo de teste e em modo de
demonstração de telas. Como esses ids vêm de outro repositório, quem guarda o
acordo é `tests/prova-remoteid.py`: ele lista os nomes e confere que os dois
manifestos os liberam.

## Duas instâncias do mesmo Flatpak não compartilham `$XDG_RUNTIME_DIR`

O RemoteID é certificado em nuvem: o módulo PKCS#11 dele não fala com a
Certisign, ele manda o digest para o `remoteid-app` por um socket UNIX, e é o
aplicativo que pede o PIN e o código do autenticador. Os dois precisam concordar
sobre onde está esse socket, e o padrão do RemoteID é
`$XDG_RUNTIME_DIR/remoteid.sock`.

Dentro de um Flatpak, esse caminho é uma armadilha. Quem abre o módulo é a ponte
que o p11-kit do host inicia, ou um assinador que o navegador executou, ou o
PJeOffice: cada um deles é uma instância separada de `flatpak run`. O
`remoteid-app` é mais uma. E o `$XDG_RUNTIME_DIR` que se vê lá dentro é privado
de cada instância — o caminho é o mesmo, o diretório não. Cada lado cria o seu
socket, e nenhum acha o outro.

O sintoma é o pior tipo que este projeto conhece, porque ele não parece um
problema de encanamento: o certificado **aparece**, o navegador lista o token, a
janela do aplicativo diz que está tudo certo, e só a assinatura falha, com erro
de dispositivo. Nada aponta para o socket.

O que atravessa é o diretório de **dados**: `~/.var/app/<id>/data` tem o mesmo
caminho absoluto dentro e fora do sandbox, é o mesmo arquivo em todas as
instâncias, e é alcançável também de fora. É a mesma propriedade de que o
publicador depende para os atalhos dos navegadores em Flatpak (ver *Um sandbox
não enxerga `~/.var/app/<id>` inteiro*). Daí `REMOTEID_SOCKET` apontar para lá,
em `preparar_remoteid()` (`src/comum-pkcs11.sh`) e em `ui/preparar-drivers.sh`.

E isso **não** pode ser feito no `preparar.sh` da extensão: ele é *executado*,
não incluído, então a variável que ele exportar morre com ele.

## O `/tmp` do sandbox também é de cada instância, e o modo de teste mora nele

O RemoteID tem um interruptor só para o modo de teste, `TEST_URL`, e com ela
ligada ele reloca o estado inteiro para `/tmp/remoteid-teste` — o aplicativo, a
linha de comando e o módulo, todos juntos, sem consultar `REMOTEID_HOME`. Fora
do sandbox é uma boa decisão: um caminho, um interruptor.

Dentro, `/tmp` é um tmpfs de cada instância. O `remoteid preparar` gravaria a
conta de teste num `/tmp/remoteid-teste` que morre com o processo, e o
aplicativo abriria noutro, vazio. O teste falharia dizendo que não há
certificado, o que é verdade e não explica nada.

A saída é a raiz gravável do sandbox (ver *A raiz do sandbox é gravável, e
`/usr` não*): o preparo cria `/tmp/remoteid-teste` como **link** para os dados
do aplicativo, a cada execução. O interruptor continua sendo um só, e agora vale
dos dois lados da fronteira.

Pelo mesmo motivo, o interruptor aqui é um **arquivo** e não uma variável de
ambiente: a ponte que o p11-kit inicia sob demanda e o assinador que o navegador
executa não herdam o ambiente de terminal nenhum. Ver `adv-br-remoteid teste`.

## Um caminho de módulo vale dentro de um sandbox só

`/pkcs11/adv-br.so` é criado pelo **lançador deste pacote**, na raiz tmpfs do
sandbox dele. Digitá-lo na configuração de outro Flatpak (o PJeOffice, por
exemplo) aponta para um arquivo que lá não existe, e o sintoma é uma lista de
certificados vazia, sem erro.

A saída não foi ensinar dois caminhos: foi o lançador do PJeOffice passar a
criar os **mesmos** atalhos, com os mesmos nomes. Quem usa as duas coisas
aprende um caminho só.

Com uma diferença que importa: lá o `/pkcs11/adv-br.so` aponta para o **shim**
(`/app/lib/pkcs11/pkcs11.so`), não para o `p11-kit-proxy` do runtime. O
`DriverSetup.create()` do signer4j chama `library.toRealPath()` antes de gravar
o caminho em `~/.pjeoffice-pro/pjeoffice-pro.config`; apontando para o proxy, o
que ficaria gravado é `libp11-kit.so.0.4.10`, e a primeira atualização de
runtime que mude esse número tira o driver do usuário sem dizer nada. O shim é
arquivo regular, e o caminho real dele é ele mesmo.

## A leitora é disputada, e o teste sofre mais que o uso real

O `p11-kit` do host inicia um `adv-br-pkcs11` por módulo, **sob demanda e por
cliente**: com um navegador aberto e o `p11-kit-server.socket` ligado, é comum
haver cinco ou mais vivos ao mesmo tempo, cada um com sessão na leitora. Alguns
drivers serializam o acesso ao cartão, então uma leitura que normalmente leva
segundos pode passar de um minuto enquanto os outros seguram o token.

Medido no mesmo minuto, com o mesmo token e o mesmo assinador:

| como | tempo |
|---|---|
| `pkcs11Modules` com os módulos explícitos | 27 s |
| `pkcs11Modules` com `/pkcs11/adv-br.so` | 3,5 s |
| o mesmo, antes, com processos remotos ocupados | > 150 s |

A conclusão que **não** se deve tirar disso é que um caminho é melhor que o
outro. Foi o que se tentou aqui, e o uso real desmentiu: assinar num site pelo
Lacuna Web PKI, com `/pkcs11/adv-br.so`, funcionou de primeira. O que a
variação mede é contenção, não caminho.

Para um teste comparável, feche os navegadores antes.

## As duas pontas precisam estar na mesma série do p11-kit

A ponte é o remoting do p11-kit, e o que trafega no pipe é a tabela de funções
PKCS#11 serializada. Host em 0.26 contra o outro lado em 0.25: os slots
enumeram, o PIN é aceito, o `C_FindObjects` devolve as chaves, e **todo**
`C_SignInit` falha, com `CKR_DEVICE_ERROR`. Nenhum mecanismo escapa.

Isso não atinge só assinar documento. A autenticação por certificado também
exige uma assinatura, no `CertificateVerify` do handshake TLS, então Projudi,
eproc e login do gov.br param de autenticar **com a lista de certificados
aparecendo normalmente**. É o modo de falha mais caro que existe aqui: tudo
parece certo até o último passo.

A causa está a montante, na 0.26.0 ("pkcs11: Update PKCS11 headers to version
3.2"), e não é acidente isolado: a 0.25.8 foi um "rpc: Unbreak protocol
compatibility by reverting …".

Aqui o runtime é fixo (série 0.26), então quem varia é o host: **Debian trixie e
Ubuntu 24.04 trazem 0.25** e caem nesse caso.

**O que o projeto faz com isso.** O Flatpak não escolhe runtime pelo host, e o
runtime não vai voltar de série. Como este pacote é sempre construído na
máquina de quem instala, o `./instalar.sh` compara as duas séries **antes de
construir** e, quando divergem, acrescenta ao build um p11-kit da série do
host, isolado em `/app/lib/p11kit-compat`. Só o processo da ponte
(`adv-br-pkcs11`) o usa; o resto do pacote continua com o do runtime, e não
precisa concordar com ninguém porque não atravessa pipe nenhum.

As versões por série estão em `packaging/p11kit-series.txt`, com sha256. O
módulo gerado é `packaging/p11kit-compat.yml`, que no caso normal (host e
runtime na mesma série) não faz nada.

Dois detalhes que custaram uma rodada cada:

- o p11-kit compilado com `prefix` próprio procura a configuração **dentro do
  prefixo**. Sem `-Dsysconfdir=/etc` ele não enxerga os módulos que o lançador
  registra;
- o campo `library-version` do `p11-kit list-modules` é a versão reportada pelo
  **módulo**, não a da `libp11-kit` do processo. Perguntar ao módulo de
  confiança do runtime devolve `0.26` mesmo com a ponte usando uma 0.25, o que
  parece certo e é exatamente o engano a evitar. A pergunta tem de ir ao trust
  do p11-kit que a ponte de fato usa.

A série sai do próprio p11-kit (`library-version` do módulo de confiança), e
não do nome do arquivo: o soname é `libp11-kit.so.0.4.1` na 0.25 e `0.4.10`/
`0.4.11` na 0.26, o que distingue por acaso e ordena errado como texto.

## Um módulo que não carrega desaparece da contagem, calado

Todos os `.module` são registrados com `critical: no`, para que um driver
quebrado não derrube os outros. O preço é que um driver que **não sobe** não
produz erro nenhum: ele apenas some da saída do `p11-kit list-modules`.

O único sinal é a diferença entre o número de módulos registrados e o de
módulos listados. Foi assim que se detectou o SerproID falhando antes de
existir o diretório de certificados que ele exige. O `./diagnostico.sh` compara
os dois.

E a guarda precisa de um **piso absoluto**: comparar duas contagens derivadas
da mesma fonte passa feliz com `0` e `0`.

## O SafeNet derruba a JVM quando ela sai, não quando você usa

Depois que o SunPKCS11 foi inicializado, encerrar o processo dá SIGSEGV dentro
de `SCardCancel`, numa thread nativa que o próprio driver criou (`si_code:
SI_KERNEL`, `si_addr: 0x0`, pilha em `libpcsclite_real.so.1`). **Em uso não há
problema**: a inicialização leva 160 ms e o assinador opera normalmente. O
crash é depois de o trabalho terminar.

O que custa tempo é o disfarce: por padrão a JVM escreve um core dump antes de
morrer, e a espera faz o crash parecer travamento. O lançador do PJeOffice
passa `-XX:-CreateCoredumpOnCrash` por isso. Com ele, o crash aparece em
segundos, com a pilha.

E cuidado com o diagnóstico por eliminação: a primeira suspeita foi o SerproID,
que foi removido e o sintoma continuou. Só isolando módulo a módulo
(`rm /etc/pkcs11/modules/*.module` e recolocando um por vez) o SafeNet
apareceu.

## `cmd | grep -q` mente quando há `pipefail`

O `grep -q` sai assim que encontra a primeira ocorrência. Quem estava
escrevendo do outro lado do cano leva SIGPIPE, termina com 141, e com
`set -o pipefail` o pipeline inteiro devolve 141, ou seja, **falha**, mesmo
tendo encontrado o que procurava.

O efeito é cruel porque depende da **posição** da linha que casou: se ela vier
por último, o produtor termina antes do grep sair e tudo passa. Aqui um teste
que procurava duas ferramentas encontrou a que aparecia por último e "não
encontrou" a que aparecia primeiro, no mesmo comando e com a mesma saída.

A correção é capturar antes de filtrar:

```sh
printf '%s\n' "$(cmd)" | grep -q padrao
```

Vale para todo `| head`, `| grep -q`, `| head -1` num script com `pipefail`.

## `pkill -f` mata o próprio shell

`pkill -f "pjeoffice-pro.jar"` casa a linha de comando de quem está rodando o
`pkill`. Custou três execuções abortadas com código 144 até ficar visível. Use
`pgrep -x java`, um padrão que não apareça no comando que o executa, ou o truque
do colchete (`'[a]dv-br-pkcs11'`), que é o que este repositório faz.

Para encerrar o PJeOffice, o certo é `flatpak kill io.github.llawli.AdvBr`: o
shutdown hook roda e o log fecha com `App closed`.

## `couldn't load token info` não é falha

O p11-kit imprime isso quando um driver diz que o cartão presente na leitora não
é dele, o que acontece sempre que há mais de um driver instalado. O que importa
é `couldn't load module`, `couldn't open` e `couldn't initialize`.

## Não teste PIN em token de hardware

`pdfsig -list-nicks` pede senha, e cada tentativa errada gasta uma das poucas
que o token tem. Um token com a flag `user-pin-final-try` é bloqueado pela
tentativa seguinte. Listar tokens não exige login: pare aí. É por isso que as
provas deste repositório nunca autenticam.
