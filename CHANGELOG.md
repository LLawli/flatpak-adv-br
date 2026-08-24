# Registro de mudanças

Todas as mudanças relevantes deste projeto. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e a numeração
segue o [SemVer](https://semver.org/lang/pt-BR/).

## [0.1.0] — 2026-08-22

Primeira versão. O `sora-adv-br` com Flatpak no lugar do distrobox.

### Adicionado

- Flatpak `io.github.llawli.AdvBr` com OpenSC, pcsc-lite (cliente) e os três
  assinadores: Lacuna Web PKI 2.16.0, Softplan WebSigner 2.15.0 e Certisign
  WebSigner 2.17.7.
- Ponte PKCS#11 para o host por `.module` com `remote: |flatpak run`, um
  processo por driver.
- Publicação de *native messaging* para navegadores do host e em Flatpak, com
  atalhos separados (`flatpak run` e `flatpak-spawn --host flatpak run`).
- Registro nos bancos NSS sem `nss-tools`: `host/nssdb.py` edita o
  `pkcs11.txt`. Bancos que vivem no home real recebem os dois módulos, o do
  host e o do sandbox, o que faz o mesmo arquivo servir ao Papers em Flatpak e
  aos programas do host.
- Extensões de driver SafeSign, SafeNet e SerproID, herdando a convenção do
  PjeOffice-flatpak.
- `/pkcs11/adv-br.so` dentro do sandbox: um caminho só, digitável na aba
  "Cripto Dispositivos" das extensões, que responde por todos os drivers.
- `./diagnostico.sh`, `./host/testar-pkcs11.sh`, `./host/testar-assinador.sh` e
  `./tests/testar.sh`.
- Atalho de menu para o aplicativo do SerproID: a extensão traz
  `atalhos/<nome>.{desktop,png}` e o `./host/publicar.sh` escreve no host, com
  ícone. O Flatpak não exporta arquivos de extensão, então a travessia é
  explícita.
- O ponto de extensão `io.github.llawli.AdvBr.Driver` é público: outros
  pacotes o declaram e recebem os mesmos drivers. O PjeOffice-flatpak deixou de
  ter extensões próprias por causa disso — uma cópia só no disco, uma correção
  por armadilha.
- **PJeOffice Pro** dentro do pacote, com atalho próprio no menu
  (`io.github.llawli.AdvBr.PJeOffice.desktop`) e o shim PKCS#11 que ele exige.
  Passa a existir em dois lugares — aqui e no pacote PjeOffice-flatpak —, que
  são o mesmo assinador; instalar os dois é redundante.
- `./instalar.sh --refazer`, para reconstruir uma extensão já instalada.
- Compatibilidade automática de série do p11-kit: quando o host está numa série
  diferente da do runtime (Debian trixie e Ubuntu 24.04 trazem 0.25 contra a
  0.26 do runtime), o `./instalar.sh` compila um p11-kit da série do host,
  isolado, e só a ponte o usa. Sem isso, o token aparece, o PIN é aceito e toda
  assinatura falha.
- Instalação a partir de um repositório local quando o `--install` do
  flatpak-builder falha por causa do remoto — a construção já terminou, e
  perder meia hora de compilação por um Flathub lento no último passo é
  desnecessário.
- Comparação das séries do p11-kit do host e do pacote no `./diagnostico.sh`.
  Divergir aí é o modo de falha mais caro do projeto — autentica e não assina —
  e o host varia: Debian trixie e Ubuntu 24.04 trazem 0.25 contra a 0.26 do
  runtime.
- Conferência de que todo módulo registrado no sandbox é também carregado. Com
  `critical: no`, um driver que não sobe some da listagem sem erro nenhum.
- CI (`.github/workflows/ci.yml`): shellcheck com a versão impressa no log,
  compilação dos scripts Python, manifestos, existência das fontes citadas, e
  validação do metainfo e do `.desktop`.
- `docs/ARMADILHAS.md` com o que foi medido.

### Corrigido

- O manifesto de native messaging era escrito no formato do Firefox para
  navegadores da família Chromium, por uma colisão de variável em shell. O
  navegador ignora o arquivo em silêncio e a extensão diz que o assinador não
  está instalado. O `./diagnostico.sh` passou a conferir o formato de cada
  manifesto publicado.
- O atalho para navegador em Flatpak ficava em `~/.var/app/<id>/.local/bin/`,
  que o sandbox não enxerga: o Flatpak monta só os diretórios XDG e o que o app
  declara em `persistent`. Ele passou para `~/.var/app/<id>/data/adv-br/`, cujo
  caminho absoluto é o mesmo dentro e fora do sandbox — o que também deixa o
  diagnóstico conferir, do host, se o atalho existe e é executável.

- `libserproidp11.so` não declarava a `libgcc_s` em `DT_NEEDED` e falhava com
  `undefined symbol: _Unwind_Resume_or_Rethrow` em qualquer processo que a
  abrisse com `dlopen`. O manifesto da extensão corrige com
  `patchelf --add-needed`, e as três extensões passaram a conferir, no build,
  que a biblioteca **carrega** — não só que o `ldd` não reclama.
