#!/usr/bin/env bash
# Ponta a ponta dos assinadores: o Âncora publica um assinador de teste
# (Assinador.Teste, assinador-teste.py) para um navegador nativo e para o
# Firefox Flatpak de uma home falsa. O assinador, chamado como o navegador o
# chamaria, carrega libaetpkss.so.3 pelo nome e precisa enxergar o token uma
# vez: no sandbox do Âncora (nativo) e no sandbox do Firefox de verdade.
#
# O servidor do sistema é um `p11-kit server` do host lendo a home falsa, como
# em aplicativos.sh.
set -euo pipefail
source "$(dirname "$0")/comum.sh"

flatpak info org.mozilla.firefox >/dev/null 2>&1 || { echo "precisa do org.mozilla.firefox instalado"; exit 1; }

HOST_NM=dev.lukakuuhaku.ancora.teste
FF=$FALSA/.var/app/org.mozilla.firefox
BRAVE=$FALSA/.config/BraveSoftware/Brave-Browser
SISTEMA_DIR=/run/user/$(id -u)/ancora-e2e
SISTEMA=$SISTEMA_DIR/sistema
rm -rf "$FF" "$FALSA/.config/BraveSoftware" "$FALSA/.local/share/ancora" "$FALSA/.local/share/flatpak/overrides" "$BANCADA/dados"
mkdir -p "$FF" "$BRAVE" "$SISTEMA_DIR"

instalar SoftHSM Assinador
subir
HOME=$FALSA XDG_CONFIG_HOME=$FALSA/.config p11-kit server -f -n "$SISTEMA" pkcs11: >/dev/null 2>&1 &
PARAR+=($!)
sleep 1
export ANCORA_SERVIDOR_DO_SISTEMA=$SISTEMA

# Uma mensagem de native messaging, e a resposta decodificada.
conversar() {
    python3 -I -c '
import json, struct, subprocess, sys
corpo = json.dumps({"comando": "listar"}).encode()
p = subprocess.run(sys.argv[1:], input=struct.pack("<I", len(corpo)) + corpo, capture_output=True)
n = struct.unpack("<I", p.stdout[:4])[0] if len(p.stdout) >= 4 else 0
print(p.stdout[4:4 + n].decode() if n else json.dumps({"saida": p.stdout.decode(errors="replace"), "erro": p.stderr.decode(errors="replace")}))
' "$@"
}
vezes() { python3 -I -c 'import json,sys; print(json.load(sys.stdin).get("tokens", []).count("HUBTESTE"))'; }
campo() { python3 -I -c "import json,sys; print(json.load(sys.stdin).get('$1'))"; }

ancora assinadores publicar | sed 's/^/     /'
RAIZ=$(dirname "$(dirname "$(dirname "$(flatpak info --show-location dev.lukakuuhaku.Ancora.Assinador.Teste)")")")

# Navegador nativo: o manifesto aponta o lançador, que chama `ancora assinar`.
NATIVO=$FALSA/.local/share/ancora/assinadores/$HOST_NM
confere "o manifesto do Brave nativo aponta o lançador" \
    "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["path"])' "$BRAVE/NativeMessagingHosts/$HOST_NM.json")" "$NATIVO"
confere "o lançador nativo chama o Âncora" "$(grep -c "flatpak run --command=ancora dev.lukakuuhaku.Ancora assinar '$HOST_NM'" "$NATIVO")" "1"
resposta=$(conversar flatpak run --env=ANCORA_SERVIDOR_DO_SISTEMA="$SISTEMA" --filesystem="$SISTEMA_DIR" \
    --command=ancora dev.lukakuuhaku.Ancora assinar "$HOST_NM" "chrome-extension://abcdefghijklmnopabcdefghijklmnop/")
echo "     nativo: $resposta"
confere "no sandbox do Âncora, o assinador vê o token uma vez" "$(echo "$resposta" | vezes)" "1"
confere "e roda com uma casa própria" "$(echo "$resposta" | campo home)" "$HOME/.var/app/dev.lukakuuhaku.Ancora/data/assinadores/$HOST_NM"

# Firefox Flatpak: o manifesto aponta o lançador dentro da pasta dele.
LANCADOR=$FF/data/ancora/$HOST_NM
for dir in .mozilla/native-messaging-hosts config/mozilla/native-messaging-hosts; do
    confere "o manifesto do Firefox em $dir aponta o lançador" \
        "$(python3 -I -c 'import json,sys; print(json.load(open(sys.argv[1]))["path"])' "$FF/$dir/$HOST_NM.json")" "$LANCADOR"
done
confere "libaetpkss.so.3 aponta o cliente do p11-kit do runtime" "$(readlink "$FF/data/ancora/lib/libaetpkss.so.3")" "/usr/lib/x86_64-linux-gnu/pkcs11/p11-kit-client.so"
confere "o Firefox ganha leitura da extensão" "$(grep -c "filesystems=.*$RAIZ:ro" "$FALSA/.local/share/flatpak/overrides/org.mozilla.firefox")" "1"
# Como o Firefox chamaria: dentro do sandbox dele, com a permissão que o
# override dá e o servidor de teste no lugar do da sessão.
resposta=$(conversar flatpak run --filesystem="$RAIZ:ro" --filesystem="$BANCADA" --filesystem="$SISTEMA_DIR" \
    --command=sh org.mozilla.firefox -c "export P11_KIT_SERVER_ADDRESS=unix:path=$SISTEMA; exec '$LANCADOR' teste@ancora")
echo "     Firefox: $resposta"
confere "no sandbox do Firefox, o assinador vê o token uma vez" "$(echo "$resposta" | vezes)" "1"
confere "e roda com uma casa própria" "$(echo "$resposta" | campo home)" "$FF/data/ancora/casa/$HOST_NM"

ancora assinadores despublicar
# As pastas do Âncora somem junto, e o find reclama delas: o erro dele não conta.
restos=$({ find "$FF" "$BRAVE" "$FALSA/.local/share/ancora" \( -name "$HOST_NM*" -o -name 'libaetpkss*' \) 2>/dev/null || true; } | wc -l)
confere "despublicar apaga manifestos, lançadores e links" "$restos" "0"
confere "e tira a permissão que deu ao Firefox" "$( [ -e "$FALSA/.local/share/flatpak/overrides/org.mozilla.firefox" ] && echo ficou || echo saiu)" "saiu"

kill -TERM "$(pid_servico)"; sleep 3
remover SoftHSM Assinador
terminar
