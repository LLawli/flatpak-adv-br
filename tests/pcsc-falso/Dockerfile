# Um pcscd com leitoras e cartões virtuais (vsmartcard), para testar os drivers
# sem token. Como usar: docs/desenvolvimento.md, "Testar sem o token".
FROM debian:trixie
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends \
        pcscd vsmartcard-vpcd vsmartcard-vpicc python3-virtualsmartcard \
        python3-pycryptodome procps pcsc-tools \
    && rm -rf /var/lib/apt/lists/*
# O pacote do Debian põe o módulo num lugar que o Python não procura, e o vicc
# importa o pycryptodome pelo nome antigo (Crypto).
RUN ln -s Cryptodome /usr/lib/python3/dist-packages/Crypto
ENV PYTHONPATH=/usr/lib/python3/site-packages/virtualsmartcard
CMD ["sh", "-c", "pcscd -f --disable-polkit & sleep 2; chmod 777 /run/pcscd /run/pcscd/pcscd.comm; wait"]
