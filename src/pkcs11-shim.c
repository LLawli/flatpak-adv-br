/*
 * pkcs11.so — a única biblioteca PKCS#11 que o PJeOffice carrega no sandbox.
 *
 * O assinador aceita a variável PKCS11_DRIVER como diretório e carrega dali um
 * arquivo chamado "pkcs11.so". Quem faz o trabalho é o p11-kit-proxy do
 * runtime, que agrega todos os módulos configurados em /etc/pkcs11/modules.
 *
 * Um symlink para o proxy resolveria — se o signer4j não canonicasse o caminho
 * com toRealPath() antes de gravá-lo em ~/.pjeoffice-pro/pjeoffice-pro.config.
 * O que fica gravado então é o alvo final do symlink, hoje
 * libp11-kit.so.0.4.10, e a próxima atualização do runtime que mude esse
 * número deixa o usuário sem driver, silenciosamente, com uma configuração que
 * parece correta. Este arquivo é regular: seu caminho canônico é ele mesmo,
 * pertence ao app e só muda quando o app muda.
 *
 * Copiar o proxy para cá também daria um caminho estável, mas congelaria uma
 * cópia da biblioteca que deixaria de receber as correções do runtime. Daí o
 * repasse: todo o código continua sendo o do runtime, carregado por soname.
 *
 * O PKCS#11 exige um único ponto de entrada, C_GetFunctionList, e a partir dele
 * o consumidor recebe ponteiros para as funções do próprio proxy. C_GetInterface
 * e C_GetInterfaceList são a forma 3.0 do mesmo mecanismo. Os tipos aqui são
 * ponteiros opacos de propósito: nada nesta unidade de compilação precisa
 * conhecer as estruturas do PKCS#11, e assim ela não depende dos cabeçalhos.
 */
#include <dlfcn.h>
#include <stddef.h>

typedef unsigned long ck_rv_t;

#define CKR_FUNCTION_FAILED 0x00000006UL

/* Sem caminho: resolvido pelo soname, o que mantém o shim indiferente ao
 * diretório multiarch do runtime e à arquitetura. */
#define PROXY_SONAME "p11-kit-proxy.so"

static void *proxy(void)
{
    static void *handle;
    if (handle == NULL)
        handle = dlopen(PROXY_SONAME, RTLD_LAZY | RTLD_LOCAL);
    return handle;
}

static void *repassar(const char *nome)
{
    void *handle = proxy();
    return handle == NULL ? NULL : dlsym(handle, nome);
}

ck_rv_t C_GetFunctionList(void *lista)
{
    ck_rv_t (*f)(void *) = repassar("C_GetFunctionList");
    return f == NULL ? CKR_FUNCTION_FAILED : f(lista);
}

ck_rv_t C_GetInterfaceList(void *lista, void *contagem)
{
    ck_rv_t (*f)(void *, void *) = repassar("C_GetInterfaceList");
    return f == NULL ? CKR_FUNCTION_FAILED : f(lista, contagem);
}

ck_rv_t C_GetInterface(void *nome, void *versao, void *interface, unsigned long flags)
{
    ck_rv_t (*f)(void *, void *, void *, unsigned long) = repassar("C_GetInterface");
    return f == NULL ? CKR_FUNCTION_FAILED : f(nome, versao, interface, flags);
}
