#!/usr/bin/env python3
"""Gera um PDF de uma página para testar assinatura digital no Papers.

Escrito à mão, sem biblioteca: o projeto não precisa de uma dependência nova
para produzir uma folha com texto, e um PDF gerado aqui é um PDF que se
entende inteiro quando algo der errado na hora de assinar.

Uso: pdf-de-teste.py <arquivo.pdf> [titular]
"""
import sys
import zlib

LARGURA, ALTURA = 595, 842          # A4 em pontos
MARGEM = 62


def texto_pdf(s):
    """Codifica em WinAnsi e escapa o que o PDF trata como sintaxe."""
    bruto = s.encode("cp1252", "replace")
    for antes, depois in ((b"\\", b"\\\\"), (b"(", b"\\("), (b")", b"\\)")):
        bruto = bruto.replace(antes, depois)
    return bruto


def linhas_de_conteudo(titular, quando):
    y = ALTURA - MARGEM - 30
    partes = []

    def escrever(fonte, tamanho, x, y, s):
        partes.append(b"BT /" + fonte + b" %d Tf %d %d Td (" % (tamanho, x, y)
                      + texto_pdf(s) + b") Tj ET\n")

    escrever(b"F2", 20, MARGEM, y, "Teste de assinatura digital")
    y -= 26
    escrever(b"F1", 10, MARGEM, y,
             "Documento gerado pelo flatpak-adv-br para verificar a assinatura "
             "de PDF no Papers.")
    y -= 40

    for paragrafo in (
        "Este arquivo nao tem valor juridico. Ele existe para responder a uma",
        "pergunta so: o Papers, rodando em Flatpak, enxerga o certificado do",
        "token e consegue assinar com ele?",
        "",
        "Como assinar, no Papers:",
        "",
        "   1. abra o menu (canto superior direito) e escolha Assinatura digital;",
        "   2. arraste um retangulo sobre a area tracejada abaixo;",
        "   3. escolha o seu certificado na lista, digite o PIN do token e salve",
        "      o documento assinado com outro nome.",
        "",
        "Se a lista de certificados vier vazia com o token espetado, o problema",
        "esta na travessia, e nao no Papers: rode ./host/testar-pkcs11.sh, que",
        "mede exatamente isso.",
        "",
        "Depois de assinar, reabra o arquivo salvo: o Papers mostra a assinatura",
        "e diz de quem e.",
    ):
        if paragrafo:
            escrever(b"F1", 11, MARGEM, y, paragrafo)
        y -= 17

    y -= 24
    caixa_altura = 96
    caixa_y = y - caixa_altura
    partes.append(b"0.55 0.55 0.55 RG 1 w [5 4] 0 d\n")
    partes.append(b"%d %d %d %d re S\n"
                  % (MARGEM, caixa_y, LARGURA - 2 * MARGEM, caixa_altura))
    partes.append(b"[] 0 d\n")
    escrever(b"F1", 9, MARGEM + 10, caixa_y + caixa_altura - 18,
             "assine aqui dentro")

    escrever(b"F1", 9, MARGEM, caixa_y - 34, "titular esperado: %s" % titular)
    escrever(b"F1", 9, MARGEM, caixa_y - 48, "gerado em: %s" % quando)
    return b"".join(partes)


def montar(conteudo):
    fluxo = zlib.compress(conteudo)
    objetos = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 %d %d] "
        b"/Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>"
        % (LARGURA, ALTURA),
        b"<< /Length %d /Filter /FlateDecode >>\nstream\n" % len(fluxo)
        + fluxo + b"\nendstream",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica "
        b"/Encoding /WinAnsiEncoding >>",
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold "
        b"/Encoding /WinAnsiEncoding >>",
    ]

    saida = bytearray(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n")
    posicoes = []
    for numero, corpo in enumerate(objetos, start=1):
        posicoes.append(len(saida))
        saida += b"%d 0 obj\n" % numero + corpo + b"\nendobj\n"

    inicio_xref = len(saida)
    saida += b"xref\n0 %d\n" % (len(objetos) + 1)
    saida += b"0000000000 65535 f \n"
    for posicao in posicoes:
        saida += b"%010d 00000 n \n" % posicao
    saida += (b"trailer\n<< /Size %d /Root 1 0 R >>\nstartxref\n%d\n%%%%EOF\n"
              % (len(objetos) + 1, inicio_xref))
    return bytes(saida)


def main(argv):
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    destino = argv[1]
    titular = argv[2] if len(argv) > 2 else "o titular do token"
    # A data entra como argumento do chamador quando importa; aqui basta a do
    # sistema, e ela e' so' rotulo dentro da folha.
    import datetime
    quando = datetime.datetime.now().strftime("%d/%m/%Y %H:%M")
    with open(destino, "wb") as f:
        f.write(montar(linhas_de_conteudo(titular, quando)))
    print(destino)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
