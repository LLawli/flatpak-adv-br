/* Um módulo PKCS#11 sem nenhum slot, como o driver de uma leitora sem
 * cartão: C_GetSlotList responde zero, e qualquer buffer serve. */
#include <p11-kit/pkcs11.h>
#include <stddef.h>
#include <string.h>

static CK_RV nao(void) { return CKR_FUNCTION_NOT_SUPPORTED; }
static CK_RV iniciar(CK_VOID_PTR a) { (void)a; return CKR_OK; }
static CK_RV finalizar(CK_VOID_PTR a) { (void)a; return CKR_OK; }
static CK_RV info(CK_INFO_PTR i) {
    memset(i, ' ', sizeof *i);
    i->cryptokiVersion.major = 2; i->cryptokiVersion.minor = 40;
    i->flags = 0; i->libraryVersion.major = 1; i->libraryVersion.minor = 0;
    return CKR_OK;
}
static CK_RV slots(CK_BBOOL presente, CK_SLOT_ID_PTR lista, CK_ULONG_PTR n) {
    (void)presente; (void)lista;
    *n = 0;
    return CKR_OK;
}
static CK_FUNCTION_LIST tabela;
CK_RV C_GetFunctionList(CK_FUNCTION_LIST_PTR_PTR saida) {
    void (**f)(void) = (void (**)(void))&tabela.C_Initialize;
    size_t n = (sizeof tabela - offsetof(CK_FUNCTION_LIST, C_Initialize)) / sizeof *f;
    for (size_t i = 0; i < n; i++) f[i] = (void (*)(void))nao;
    tabela.version.major = 2; tabela.version.minor = 40;
    tabela.C_Initialize = iniciar;
    tabela.C_Finalize = finalizar;
    tabela.C_GetInfo = info;
    tabela.C_GetFunctionList = C_GetFunctionList;
    tabela.C_GetSlotList = slots;
    *saida = &tabela;
    return CKR_OK;
}
