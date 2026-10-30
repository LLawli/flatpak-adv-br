/*
 * O token-sentinela do Âncora: um módulo PKCS#11 com um slot e um token
 * chamado "Âncora: verificação", sem PIN, sem objetos e sem mecanismos.
 *
 * Ele é publicado só enquanto o Âncora verifica um consumidor: quantas vezes
 * o sentinela aparece no NSS de um navegador é quantas vezes um token de
 * verdade apareceria. Assim a verificação funciona sem token espetado e
 * nunca encosta no token da pessoa. Ver src/verificacao.rs.
 */
#include <p11-kit/pkcs11.h>
#include <stddef.h>
#include <string.h>

#define SLOT 1
#define ROTULO "Âncora: verificação"
#define FABRICANTE "Âncora"

/* Campos de texto do PKCS#11: tamanho fixo, completados com espaços. */
static void preencher(unsigned char *destino, size_t tamanho, const char *texto)
{
    size_t n = strlen(texto);
    memset(destino, ' ', tamanho);
    memcpy(destino, texto, n < tamanho ? n : tamanho);
}

static CK_RV nao(void) { return CKR_FUNCTION_NOT_SUPPORTED; }

static CK_RV iniciar(CK_VOID_PTR a) { (void)a; return CKR_OK; }
static CK_RV finalizar(CK_VOID_PTR a) { (void)a; return CKR_OK; }

static CK_RV informacao(CK_INFO_PTR i)
{
    memset(i, 0, sizeof *i);
    i->cryptokiVersion.major = 2;
    i->cryptokiVersion.minor = 40;
    preencher(i->manufacturerID, sizeof i->manufacturerID, FABRICANTE);
    preencher(i->libraryDescription, sizeof i->libraryDescription, "Token-sentinela");
    i->libraryVersion.major = 1;
    return CKR_OK;
}

static CK_RV slots(CK_BBOOL presente, CK_SLOT_ID_PTR lista, CK_ULONG_PTR n)
{
    (void)presente;
    if (n == NULL)
        return CKR_ARGUMENTS_BAD;
    if (lista != NULL) {
        if (*n < 1) {
            *n = 1;
            return CKR_BUFFER_TOO_SMALL;
        }
        lista[0] = SLOT;
    }
    *n = 1;
    return CKR_OK;
}

static CK_RV slot(CK_SLOT_ID s, CK_SLOT_INFO_PTR i)
{
    if (s != SLOT)
        return CKR_SLOT_ID_INVALID;
    memset(i, 0, sizeof *i);
    preencher(i->slotDescription, sizeof i->slotDescription, FABRICANTE);
    preencher(i->manufacturerID, sizeof i->manufacturerID, FABRICANTE);
    i->flags = CKF_TOKEN_PRESENT;
    return CKR_OK;
}

static CK_RV token(CK_SLOT_ID s, CK_TOKEN_INFO_PTR i)
{
    if (s != SLOT)
        return CKR_SLOT_ID_INVALID;
    memset(i, 0, sizeof *i);
    preencher(i->label, sizeof i->label, ROTULO);
    preencher(i->manufacturerID, sizeof i->manufacturerID, FABRICANTE);
    preencher(i->model, sizeof i->model, "sentinela");
    preencher(i->serialNumber, sizeof i->serialNumber, "1");
    /* Sem CKF_LOGIN_REQUIRED: o navegador nunca pede PIN para ele. */
    i->flags = CKF_TOKEN_INITIALIZED | CKF_WRITE_PROTECTED;
    i->ulMaxSessionCount = CK_EFFECTIVELY_INFINITE;
    i->ulMaxRwSessionCount = 0;
    i->ulTotalPublicMemory = CK_UNAVAILABLE_INFORMATION;
    i->ulFreePublicMemory = CK_UNAVAILABLE_INFORMATION;
    i->ulTotalPrivateMemory = CK_UNAVAILABLE_INFORMATION;
    i->ulFreePrivateMemory = CK_UNAVAILABLE_INFORMATION;
    return CKR_OK;
}

static CK_RV mecanismos(CK_SLOT_ID s, CK_MECHANISM_TYPE_PTR lista, CK_ULONG_PTR n)
{
    (void)lista;
    if (s != SLOT)
        return CKR_SLOT_ID_INVALID;
    *n = 0;
    return CKR_OK;
}

static CK_RV abrir(CK_SLOT_ID s, CK_FLAGS flags, CK_VOID_PTR a, CK_NOTIFY aviso,
                   CK_SESSION_HANDLE_PTR sessao)
{
    (void)a;
    (void)aviso;
    if (s != SLOT)
        return CKR_SLOT_ID_INVALID;
    if (!(flags & CKF_SERIAL_SESSION))
        return CKR_SESSION_PARALLEL_NOT_SUPPORTED;
    if (flags & CKF_RW_SESSION)
        return CKR_TOKEN_WRITE_PROTECTED;
    *sessao = 1;
    return CKR_OK;
}

static CK_RV fechar(CK_SESSION_HANDLE h) { (void)h; return CKR_OK; }
static CK_RV fechar_todas(CK_SLOT_ID s) { (void)s; return CKR_OK; }

static CK_RV sessao(CK_SESSION_HANDLE h, CK_SESSION_INFO_PTR i)
{
    (void)h;
    i->slotID = SLOT;
    i->state = CKS_RO_PUBLIC_SESSION;
    i->flags = CKF_SERIAL_SESSION;
    i->ulDeviceError = 0;
    return CKR_OK;
}

static CK_RV busca_inicio(CK_SESSION_HANDLE h, CK_ATTRIBUTE_PTR t, CK_ULONG n)
{
    (void)h;
    (void)t;
    (void)n;
    return CKR_OK;
}

static CK_RV busca(CK_SESSION_HANDLE h, CK_OBJECT_HANDLE_PTR o, CK_ULONG maximo,
                   CK_ULONG_PTR n)
{
    (void)h;
    (void)o;
    (void)maximo;
    *n = 0;
    return CKR_OK;
}

static CK_RV busca_fim(CK_SESSION_HANDLE h) { (void)h; return CKR_OK; }

static CK_FUNCTION_LIST tabela;

CK_RV C_GetFunctionList(CK_FUNCTION_LIST_PTR_PTR saida)
{
    void (**f)(void) = (void (**)(void))&tabela.C_Initialize;
    size_t n = (sizeof tabela - offsetof(CK_FUNCTION_LIST, C_Initialize)) / sizeof *f;
    for (size_t i = 0; i < n; i++)
        f[i] = (void (*)(void))nao;
    tabela.version.major = 2;
    tabela.version.minor = 40;
    tabela.C_Initialize = iniciar;
    tabela.C_Finalize = finalizar;
    tabela.C_GetInfo = informacao;
    tabela.C_GetFunctionList = C_GetFunctionList;
    tabela.C_GetSlotList = slots;
    tabela.C_GetSlotInfo = slot;
    tabela.C_GetTokenInfo = token;
    tabela.C_GetMechanismList = mecanismos;
    tabela.C_OpenSession = abrir;
    tabela.C_CloseSession = fechar;
    tabela.C_CloseAllSessions = fechar_todas;
    tabela.C_GetSessionInfo = sessao;
    tabela.C_FindObjectsInit = busca_inicio;
    tabela.C_FindObjects = busca;
    tabela.C_FindObjectsFinal = busca_fim;
    *saida = &tabela;
    return CKR_OK;
}
