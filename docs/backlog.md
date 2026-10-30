# Backlog

O que ficou decidido fazer depois, com o contexto para retomar sem
redescobrir.

## Um `p11-kit-remote` próprio

**O problema.** A supervisão (`src/supervisao.rs`) não sabe o que as outras
conexões de um driver estão fazendo. Quando a sonda passa do limite com outras
conexões abertas, o driver é tratado como ocupado: sai da publicação, mas não é
reiniciado. É o certo quando alguém está digitando o PIN num leitor com
teclado, e o errado quando o driver travou de verdade com um navegador aberto
(o navegador mantém a conexão o tempo todo). Ver `tests/e2e/supervisao.sh`.

**A ideia.** O `p11-kit server` executa `$P11_KIT_PRIVATEDIR/p11-kit-remote`
para cada conexão. Apontando essa variável para um diretório do Âncora com um
`p11-kit-remote` que execute `ancora remote "$@"`, quem carrega o driver passa
a ser o próprio Âncora, que envolve a tabela de funções PKCS#11 e registra, por
conexão, se ela está dentro de `C_Login`, `C_Sign` e afins, e desde quando. A
supervisão lê isso e separa "ocupado esperando a pessoa" de "travado". É o que
o Tessera faz (`tessera remote`, `src/remote.rs`), e o mesmo lugar resolve
outras coisas que ele resolve ali: sinais herdados ignorados, o canal do
p11-kit fora do stdout (um driver que escreve no stdout corrompe o protocolo)
e cópias estáveis dos parâmetros de mecanismo (o conserto do SerproID).

**O custo.** Envolver as ~70 funções da tabela, com testes, e conferir que o
`p11-kit server` do runtime respeita `P11_KIT_PRIVATEDIR` dentro do sandbox.

## O defeito do p11-kit: a lista de tokens para no primeiro que recusa sessão

**O defeito.** `p11_kit_remote_serve_tokens` (`rpc-server.c`) percorre os
tokens com `p11_kit_iter` e para no primeiro erro. O iterador (`iter.c`,
`move_next_session`) abre uma sessão em cada token depois de entregá-lo, mesmo
quando só pediram tokens, e encerra tudo se `C_OpenSession` falha. Os módulos
são ordenados por `priority` e depois pelo nome, e todo módulo depois do token
problemático some do `p11-kit server`, que é por onde os aplicativos Flatpak
chegam aos tokens. Medido no p11-kit 0.26.5; o `main` (lido em 2026-10-30) tem
o mesmo código. Hoje só o token-sentinela está protegido (`priority: 1000`).

**O plano.**

1. **Reproduzir fora do Âncora**, num caso mínimo para o upstream: dois
   módulos, o primeiro com um token que recusa `C_OpenSession` (um módulo de
   teste como `tests/e2e/travado.c`, ou o slot livre do SoftHSM), e
   `p11-kit server` exportando só o primeiro. Conferir o comportamento no
   `main` compilado.
2. **Propor a correção ao p11-kit** (p11-glue/p11-kit): no iterador, não abrir
   sessão quando o pedido é só de tokens, ou, ao falhar o `C_OpenSession`,
   pular o token em vez de encerrar. Abrir a issue com a reprodução e o
   patch, e acompanhar até entrar numa versão.
3. **Proteger o Âncora enquanto isso:**
   - a sonda da supervisão passa a abrir e fechar uma sessão em cada token;
     um driver cujo token recuse sessão sai da publicação, com o motivo, em
     vez de esconder os tokens dos outros;
   - na interface, um diagnóstico que compara a lista do proxy com a do
     `p11-kit server` e aponta o módulo que corta a lista, inclusive quando ele
     não é do Âncora (o OpenSC do sistema, por exemplo).
4. **Validar** em VM Fedora e Debian, com um token real e outro problemático
   ao mesmo tempo.

**Quando reabrir:** quando uma versão do p11-kit com a correção chegar aos
runtimes do Flatpak e às distribuições, a proteção do item 3 pode sair.
