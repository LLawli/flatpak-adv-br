# Registro de mudanças

Todas as mudanças relevantes deste projeto. O formato segue
[Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/), e a numeração
segue o [SemVer](https://semver.org/lang/pt-BR/).

## [0.1.0] - 2026-08-24

Primeira versão. O [sora-adv-br](https://github.com/LLawli/sora-adv-br) com
Flatpak no lugar do distrobox.

### Adicionado

- Pacote base `io.github.llawli.AdvBr`: OpenSC, a biblioteca cliente do PC/SC,
  o shim PKCS#11 e a ponte para o host. Poucos megabytes, porque tudo o que é
  opcional é extensão, instalada quando for usada:
  - drivers de token SafeSign, SafeNet e SerproID;
  - assinadores Lacuna Web PKI, Softplan WebSigner e Certisign WebSigner;
  - PJeOffice Pro, com atalho próprio no menu.

  Dá para instalar só um driver e um assinador, ou tudo, ou um hoje e outro
  amanhã: o `./instalar.sh` é idempotente e não refaz o que já está pronto.
- Ponte PKCS#11 para o host por `.module` com `remote: |flatpak run`, um
  processo por driver, o que isola o driver que derruba quem o carregou e evita
  devolver ao host as âncoras de confiança que ele já tem.
- Publicação de *native messaging* para navegadores do host e em Flatpak, com
  atalhos separados (`flatpak run` e `flatpak-spawn --host flatpak run`).
- Registro nos bancos NSS sem `nss-tools`: `host/nssdb.py` edita o
  `pkcs11.txt`. Bancos que vivem no home real recebem os dois módulos, o do
  host e o do sandbox, o que faz o mesmo arquivo servir ao Papers em Flatpak e
  aos programas do host.
- `/pkcs11/adv-br.so` dentro do sandbox: um caminho só, digitável na aba
  "Cripto Dispositivos" das extensões, que responde por todos os drivers.
- Compatibilidade automática de série do p11-kit: quando o host está numa série
  diferente da do runtime (Debian trixie e Ubuntu 24.04 trazem 0.25 contra a
  0.26 do runtime), o instalador compila um p11-kit da série do host, isolado,
  e só a ponte o usa. Sem isso, o token aparece, o PIN é aceito e toda
  assinatura falha.
- Instalação a partir de um repositório local quando o `--install` do
  flatpak-builder falha por causa do remoto. A construção já terminou, e perder
  meia hora de compilação por um Flathub lento no último passo é desnecessário.
- `./diagnostico.sh`, `./host/testar-pkcs11.sh`, `./host/testar-assinador.sh` e
  `./tests/testar.sh`.
- `docs/arquitetura.md` e `docs/ARMADILHAS.md`, com o que foi medido.
- CI estático e workflow de release por tag; `bin/release` para soltar uma.
