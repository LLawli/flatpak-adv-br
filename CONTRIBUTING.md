# Contribuindo

## O que este projeto é

Um Flatpak que reúne drivers de token e assinadores e os publica para os
navegadores, o Papers e os aplicativos que já existem na máquina. O desenho
está em [docs/arquitetura.md](docs/arquitetura.md); o que já foi medido e
enganou, em [docs/ARMADILHAS.md](docs/ARMADILHAS.md). **Leia as armadilhas
antes de mexer na camada PKCS#11** — quase toda falha ali é silenciosa.

## Antes de abrir um PR

```sh
make lint      # shellcheck em todos os scripts
make testar    # os testes do repositório; não precisam de token
```

E, se você tem um token à mão:

```sh
./diagnostico.sh
```

O CI roda o estático em cada push. Ele **não** constrói o pacote de propósito:
todo módulo baixa de site de fabricante, e um CI vermelho por
indisponibilidade alheia é um CI que ninguém olha.

## Acrescentar um driver, um assinador ou um aplicativo

Nada disso mexe no pacote base. Escreva o manifesto na pasta certa, seguindo o
README dela, e acrescente uma linha à tabela `EXTENSOES` do `instalar.sh`:

- driver de token → [drivers/README.md](drivers/README.md)
- assinador de navegador → [assinadores/README.md](assinadores/README.md)
- aplicativo com janela → [apps/README.md](apps/README.md)

Todo binário proprietário é baixado da URL do próprio fabricante, com `sha256`
fixo no manifesto. **Nada é redistribuído por este repositório**, e é por isso
que não existe um `.flatpak` pronto para baixar.

Ao contribuir, você concorda em licenciar sua contribuição sob a
**GPL-3.0-only**, como o resto do projeto.

## Estilo

- Português nos comentários, nas mensagens e na documentação. Quem usa isto é
  quem está com o token na mão às onze da noite.
- Comentário explica **por que**, não o que o código já diz. Se algo parece
  errado e não é, essa é a hora de dizer.
- Uma guarda precisa poder falhar: comparar duas contagens derivadas da mesma
  fonte passa feliz com `0` e `0`. Compare também com um piso absoluto.
- Verificação que só olha (`ldd`, contar arquivos) não substitui verificação
  que chama. As duas coisas já falharam aqui de formas diferentes.

## Commits e releases

Conventional commits, em português. A mensagem registra a decisão e o porquê —
não repita o diff.

Uma release é `bin/release X.Y.Z`, que confere o repositório, exige a seção da
versão no `CHANGELOG.md`, versiona, commita e cria a tag. O workflow de release
extrai essa seção do changelog e a usa como corpo do GitHub Release.
