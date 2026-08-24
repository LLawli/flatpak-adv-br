# Por que existe um certificado de CA neste repositório

O servidor `websigner.softplan.com.br`, de onde vem o Softplan WebSigner, serve
uma **cadeia incompleta**: no lugar do certificado intermediário ele repete o
próprio certificado do servidor. O resultado é que qualquer cliente que valide
TLS corretamente recusa o download com "unable to get local issuer
certificate", e é por isso que outros projetos baixam esse arquivo com
`curl -k` (sem verificação nenhuma).

Aqui não. `ThawteTLSRSACAG1.pem` é o intermediário que está faltando, baixado
de <http://cacerts.digicert.com/ThawteTLSRSACAG1.crt>, emitido pela
*DigiCert Global Root G2*, que é uma raiz do próprio sistema. Acrescentá-lo ao
bundle **completa** a cadeia em vez de desligar a verificação: com ele,
`openssl s_client` devolve `Verify return code: 0 (ok)`.

O `instalar.sh` monta um bundle temporário (as CAs do sistema + este arquivo) e
o passa ao `flatpak-builder` em `SSL_CERT_FILE` e `CURL_CA_BUNDLE`. Nada é
instalado no armazenamento de confiança da máquina.

Conferir:

```sh
openssl x509 -in ThawteTLSRSACAG1.pem -noout -subject -issuer
openssl verify -CAfile /etc/pki/ca-trust/extracted/pem/tls-ca-bundle.pem ThawteTLSRSACAG1.pem
```

sha256: `9f452aab7cd7684c4add57dff51dca5954372ebe2af9e6e24b7c779852dc8f05`
