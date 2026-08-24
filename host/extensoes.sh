#!/usr/bin/env bash
# A tabela do que é opcional. Carregada com "."; não roda sozinha.
#
# Fica aqui, e não dentro do instalar.sh, porque o desinstalar.sh precisa da
# mesma lista: duas cópias divergiriam no dia em que entrasse uma extensão
# nova, e o sintoma seria "instala mas não desinstala".
#
# shellcheck disable=SC2034
# (as tabelas daqui são usadas por quem carrega este arquivo, não por ele.)

# opção → manifesto da extensão
declare -A EXTENSOES=(
    [safesign]=drivers/io.github.llawli.AdvBr.Driver.SafeSign.yml
    [safenet]=drivers/io.github.llawli.AdvBr.Driver.SafeNet.yml
    [serproid]=drivers/io.github.llawli.AdvBr.Driver.SerproID.yml
    [webpki]=assinadores/io.github.llawli.AdvBr.Assinador.WebPKI.yml
    [websigner]=assinadores/io.github.llawli.AdvBr.Assinador.WebSigner.yml
    [certisign]=assinadores/io.github.llawli.AdvBr.Assinador.Certisign.yml
    [pjeoffice]=apps/io.github.llawli.AdvBr.App.PJeOffice.yml
)
DRIVERS_TODOS=(safesign safenet serproid)
ASSINADORES_TODOS=(webpki websigner certisign)

# opção → id da extensão instalada. Sai do próprio manifesto, para não haver
# uma segunda tabela para desencontrar.
id_da_extensao() { # <opção>
    local manifesto="${EXTENSOES[$1]:-}"
    [ -n "$manifesto" ] || return 1
    [ -e "$AQUI_RAIZ/$manifesto" ] || return 1
    sed -n 's/^id: *//p' "$AQUI_RAIZ/$manifesto" | head -1
}

# Aplicativos em Flatpak que este projeto pode ter tocado, e o que foi
# concedido a cada um. Usado na desinstalação para devolver as permissões ao
# estado anterior.
#
#   <id> <tipo> <valor>
PERMISSOES_CONCEDIDAS=(
    "org.mozilla.firefox filesystem xdg-run/p11-kit/pkcs11"
    "org.mozilla.firefox talk-name org.freedesktop.Flatpak"
    "io.gitlab.librewolf-community filesystem xdg-run/p11-kit/pkcs11"
    "io.gitlab.librewolf-community talk-name org.freedesktop.Flatpak"
    "one.ablaze.floorp filesystem xdg-run/p11-kit/pkcs11"
    "one.ablaze.floorp talk-name org.freedesktop.Flatpak"
    "net.waterfox.waterfox filesystem xdg-run/p11-kit/pkcs11"
    "net.waterfox.waterfox talk-name org.freedesktop.Flatpak"
    "org.mozilla.Thunderbird filesystem xdg-run/p11-kit/pkcs11"
    "org.mozilla.Thunderbird talk-name org.freedesktop.Flatpak"
    "com.google.Chrome filesystem xdg-run/p11-kit/pkcs11"
    "com.google.Chrome talk-name org.freedesktop.Flatpak"
    "org.chromium.Chromium filesystem xdg-run/p11-kit/pkcs11"
    "org.chromium.Chromium talk-name org.freedesktop.Flatpak"
    "com.brave.Browser filesystem xdg-run/p11-kit/pkcs11"
    "com.brave.Browser talk-name org.freedesktop.Flatpak"
    "com.vivaldi.Vivaldi filesystem xdg-run/p11-kit/pkcs11"
    "com.vivaldi.Vivaldi talk-name org.freedesktop.Flatpak"
    "com.microsoft.Edge filesystem xdg-run/p11-kit/pkcs11"
    "com.microsoft.Edge talk-name org.freedesktop.Flatpak"
    "com.opera.Opera filesystem xdg-run/p11-kit/pkcs11"
    "com.opera.Opera talk-name org.freedesktop.Flatpak"
    "org.gnome.Papers filesystem xdg-run/p11-kit/pkcs11"
    "org.gnome.Evince filesystem xdg-run/p11-kit/pkcs11"
    "org.libreoffice.LibreOffice filesystem xdg-run/p11-kit/pkcs11"
    "org.kde.okular filesystem xdg-run/p11-kit/pkcs11"
)
