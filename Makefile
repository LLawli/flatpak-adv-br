# Atalhos para o que o instalar.sh faz. Quem está começando deve usar
# ./instalar.sh; isto aqui é para quem já conhece o projeto.
APP_ID = io.github.llawli.AdvBr

.PHONY: ajuda instalar tudo publicar despublicar diagnostico testar lint \
        serproid pjeoffice desinstalar limpar

ajuda:
	@echo 'make instalar            constrói e instala o Flatpak, e publica'
	@echo 'make publicar            só publica para os navegadores'
	@echo 'make despublicar         desfaz a publicação'
	@echo 'make diagnostico         confere o encanamento inteiro'
	@echo 'make testar              testes do repositório (não precisam de token)'
	@echo 'make lint                shellcheck nos scripts'
	@echo 'make tudo                instala o pacote e todas as extensões'
	@echo 'make serproid            abre o aplicativo SerproID para associar o certificado'
	@echo 'make pjeoffice           abre o PJeOffice Pro'
	@echo 'make desinstalar         remove o Flatpak e desfaz a publicação'

instalar:
	./instalar.sh

tudo:
	./instalar.sh --with-tudo

publicar:
	./host/publicar.sh

despublicar:
	./host/publicar.sh --remover

diagnostico:
	./diagnostico.sh

testar:
	./tests/testar.sh

lint:
	shellcheck -S warning src/*.sh host/*.sh tests/*.sh drivers/*.sh \
	    assinadores/*.sh apps/*.sh packaging/*.sh instalar.sh diagnostico.sh \
	    bin/release

serproid:
	flatpak run --command=adv-br-ferramentas $(APP_ID) serproid

pjeoffice:
	flatpak run --command=adv-br-ferramentas $(APP_ID) pjeoffice-pro

# Remove o pacote e, com ele, todas as extensões: elas são declaradas com
# autodelete, então saem junto.
desinstalar:
	./host/publicar.sh --remover
	-flatpak uninstall --user -y $(APP_ID)

limpar:
	rm -rf build-dir build-* repo .flatpak-builder
