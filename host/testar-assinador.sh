#!/usr/bin/env bash
# Conversa com cada assinador publicado exatamente como o navegador conversaria
# e mostra o que ele responde.
#
# Serve para separar dois problemas que, na tela do site, se parecem: "a ponte
# até o assinador não está de pé" e "o assinador não está enxergando o token".
#
# Uma mensagem de native messaging é: 4 bytes de tamanho (little-endian) mais
# um JSON. É o que o python abaixo monta e lê.
set -uo pipefail

. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/comum.sh"

# O caminho de módulo que os assinadores enxergam DENTRO do sandbox: um só,
# que responde por todos os drivers registrados lá. É o mesmo caminho que se
# escreve na aba "Cripto Dispositivos" da extensão.
MODULO_NO_SANDBOX=/pkcs11/adv-br.so

# Origem exigida pelo assinador da Lacuna; os outros aceitam qualquer uma.
ORIGEM="chrome-extension://dcngeagmmhegagicpcmpinaoklddcgon/"

conversar() { # <wrapper> <json-da-mensagem>  -> imprime o JSON da resposta
    local wrapper=$1 mensagem=$2
    python3 -c '
import json, struct, sys
corpo = sys.argv[1].encode()
sys.stdout.buffer.write(struct.pack("<I", len(corpo)) + corpo)
' "$mensagem" |
        timeout "${ADV_BR_ESPERA:-150}" "$wrapper" "$ORIGEM" 2>/dev/null |
        python3 -c '
import json, struct, sys
dados = sys.stdin.buffer.read()
if not dados:
    sys.exit(1)
tamanho = struct.unpack("<I", dados[:4])[0]
print(json.dumps(json.loads(dados[4:4 + tamanho]), ensure_ascii=False))
'
}

falhou=0
achou=0

for wrapper in "$BIN_HOST/$PREFIXO_WRAPPER"*; do
    [ -x "$wrapper" ] || continue
    achou=1
    nome=$(basename "$wrapper")

    titulo "$nome"

    # O Certisign WebSigner não responde a este teste, e não é a ponte: ele sai
    # com código 1 e sem uma linha de saída também quando executado direto no
    # host, fora de qualquer sandbox, e dentro de um contêiner Debian. O
    # protocolo dele não é o dos outros dois (ele conhece getInfo,
    # listCertificates, listTokens e sign, não getVersion) e alguma outra coisa
    # que ele espera do ambiente não está sendo dita aqui. Quem o exercita de
    # verdade é a extensão no navegador.
    if ! resposta=$(conversar "$wrapper" '{"command":"getVersion","requestId":"1"}'); then
        if [ "$nome" = "${PREFIXO_WRAPPER}certisign" ]; then
            aviso "não responde a este teste, e também não responde fora do Flatpak.
      Não é a ponte. Confira pela extensão no navegador."
        else
            aviso "não respondeu. A ponte não está de pé: rode ./host/publicar.sh"
            falhou=1
        fi
        continue
    fi
    versao=$(printf '%s' "$resposta" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("response"))')
    ok "responde: versão $versao"

    if ! resposta=$(conversar "$wrapper" \
        "{\"command\":\"listCertificates\",\"requestId\":\"2\",\"pkcs11Modules\":[\"$MODULO_NO_SANDBOX\"]}"); then
        aviso "listCertificates não respondeu em ${ADV_BR_ESPERA:-150}s.
      O assinador subiu (a versão acima saiu dele), então o que demora é a
      leitura dos tokens. Duas causas, nesta ordem de probabilidade:

      1. A leitora está ocupada. Cada navegador aberto mantém processos
         'adv-br-pkcs11' com sessão no token, e alguns drivers serializam o
         acesso. Há $(pgrep -c -f '[a]dv-br-pkcs11' 2>/dev/null || echo 0) deles
         vivos agora. Feche os navegadores e repita.
      2. Um driver instalado para um token que não está espetado: o SafeNet,
         sozinho, passa de um minuto. A seção 6 do ./diagnostico.sh cronometra
         cada um.

      Este teste é mais sensível a isso do que o uso real: o navegador conversa
      com o assinador uma vez, com o token já aberto pela sessão dele."
        falhou=1
        continue
    fi
    printf '%s' "$resposta" | python3 -c '
import json, sys
resposta = json.load(sys.stdin)
if not resposta.get("success"):
    excecao = resposta.get("exception") or {}
    print("   erro: %s" % (excecao.get("message") or resposta))
    raise SystemExit(0)
certificados = resposta.get("response") or []
if not certificados:
    print("   nenhum certificado. Com o token espetado, falta o driver dele.")
    raise SystemExit(0)
for certificado in certificados:
    print("   • %s" % (certificado.get("subjectName") or "(sem nome)"))
    brasil = certificado.get("pkiBrazil") or {}
    if brasil.get("cpf") or brasil.get("oabNumero"):
        print("     CPF %s   OAB %s/%s" % (brasil.get("cpf"),
                                           brasil.get("oabNumero"),
                                           brasil.get("oabUF")))
'
done

if [ "$achou" = 0 ]; then
    aviso "nenhum assinador publicado. Rode ./host/publicar.sh"
    falhou=1
fi

printf '\n'
exit "$falhou"
