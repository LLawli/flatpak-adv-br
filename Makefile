# Atalhos para o que o instalar.sh faz. Quem está começando deve usar
# ./instalar.sh; isto aqui é para quem já conhece o projeto.
APP_ID = io.github.llawli.AdvBr

.PHONY: ajuda instalar publicar despublicar diagnostico testar lint \
        driver-safesign driver-safenet driver-serproid drivers-desinstalar \
        serproid desinstalar limpar

ajuda:
	@echo 'make instalar            constrói e instala o Flatpak, e publica'
	@echo 'make publicar            só publica para os navegadores'
	@echo 'make despublicar         desfaz a publicação'
	@echo 'make diagnostico         confere o encanamento inteiro'
	@echo 'make testar              testes do repositório (não precisam de token)'
	@echo 'make lint                shellcheck nos scripts'
	@echo 'make driver-safesign     extensão do driver SafeSign (GD Burti)'
	@echo 'make driver-safenet      extensão do driver SafeNet (eToken)'
	@echo 'make driver-serproid     extensão do SerproID (certificado em nuvem)'
	@echo 'make serproid            abre o aplicativo SerproID para associar o certificado'
	@echo 'make drivers-desinstalar remove as extensões de driver'
	@echo 'make desinstalar         remove o Flatpak e desfaz a publicação'

instalar:
	./instalar.sh

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
	    packaging/*.sh instalar.sh diagnostico.sh

driver-safesign:
	./instalar.sh --with-safesign --sem-publicar
	./host/publicar.sh

driver-safenet:
	./instalar.sh --with-safenet --sem-publicar
	./host/publicar.sh

driver-serproid:
	./instalar.sh --with-serproid --sem-publicar
	./host/publicar.sh

serproid:
	flatpak run --command=adv-br-ferramentas $(APP_ID) serproid

drivers-desinstalar:
	-flatpak uninstall --user -y $(APP_ID).Driver.SafeSign
	-flatpak uninstall --user -y $(APP_ID).Driver.SafeNet
	-flatpak uninstall --user -y $(APP_ID).Driver.SerproID
	./host/publicar.sh

desinstalar:
	./host/publicar.sh --remover
	-flatpak uninstall --user -y $(APP_ID)

limpar:
	rm -rf build-dir build-driver-* .flatpak-builder
