# Âncora

Sucessor do `~/Personal/flatpak-adv-br`, escrito do zero. Arquitetura e plano
de etapas em `docs/arquitetura.md`; construir e testar em
`docs/desenvolvimento.md`.

## Memória (ai-memory)

O `.ai-memory.toml` fixa o projeto em `ancora`. Enquanto a migração do
flatpak-adv-br não terminar, toda consulta à memória busca nos dois escopos,
porque as medições e armadilhas de tokens, p11-kit e sandbox estão lá:

```
memory_query(query=..., scopes=[
  {workspace: "default", project: "ancora"},
  {workspace: "default", project: "flatpak-adv-br"},
])
```

Escritas vão só para `ancora`. Uma armadilha nova é página nova aqui, mesmo
que complemente uma de lá.

## Regras do projeto

- Gates: `scripts/ci.sh`, rodado com teto de CPU e memória nesta máquina.
- Depois de mudar o `Cargo.lock`, rode `scripts/cargo-sources.py`.
- O app ID `dev.lukakuuhaku.Ancora` é o prefixo das extensões e não muda.
