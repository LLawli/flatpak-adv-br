/*
 * Um driver que imita um token esperando a pessoa: C_Login demora
 * ESPERA_SEGUNDOS (como um leitor com teclado esperando o PIN) segurando uma
 * trava exclusiva entre processos, como a transação do PC/SC. Com
 * ESPERA_COMPARTILHA=1, listar slots e tokens também pega a trava, como um
 * driver que fala com o cartão para listar. Para testar se a supervisão
 * confunde essa espera com um driver travado.
 */
#include <p11-kit/pkcs11.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/file.h>
#include <unistd.h>

static void preencher(unsigned char *d, size_t n, const char *s)
{
    size_t l = strlen(s);
    memset(d, ' ', n);
    memcpy(d, s, l < n ? l : n);
}

static int trava(void)
{
    char caminho[512];
    const char *dir = getenv("XDG_RUNTIME_DIR");
    snprintf(caminho, sizeof caminho, "%s/espera.trava", dir ? dir : "/tmp");
    int fd = open(caminho, O_RDWR | O_CREAT, 0600);
    if (fd >= 0)
        flock(fd, LOCK_EX);
    return fd;
}

static void soltar(int fd)
{
    if (fd >= 0) {
        flock(fd, LOCK_UN);
        close(fd);
    }
}

static int compartilha(void)
{
    const char *c = getenv("ESPERA_COMPARTILHA");
    return c && strcmp(c, "1") == 0;
}

static CK_RV nao(void) { return CKR_FUNCTION_NOT_SUPPORTED; }
static CK_RV iniciar(CK_VOID_PTR a) { (void)a; return CKR_OK; }
static CK_RV finalizar(CK_VOID_PTR a) { (void)a; return CKR_OK; }

static CK_RV informacao(CK_INFO_PTR i)
{
    memset(i, 0, sizeof *i);
    i->cryptokiVersion.major = 2;
    i->cryptokiVersion.minor = 40;
    preencher(i->manufacturerID, sizeof i->manufacturerID, "teste");
    preencher(i->libraryDescription, sizeof i->libraryDescription, "espera");
    return CKR_OK;
}

static CK_RV slots(CK_BBOOL p, CK_SLOT_ID_PTR l, CK_ULONG_PTR n)
{
    (void)p;
    int fd = compartilha() ? trava() : -1;
    if (l != NULL) {
        if (*n < 1) {
            *n = 1;
            soltar(fd);
            return CKR_BUFFER_TOO_SMALL;
        }
        l[0] = 1;
    }
    *n = 1;
    soltar(fd);
    return CKR_OK;
}

static CK_RV slot(CK_SLOT_ID s, CK_SLOT_INFO_PTR i)
{
    (void)s;
    memset(i, 0, sizeof *i);
    preencher(i->slotDescription, sizeof i->slotDescription, "leitor");
    preencher(i->manufacturerID, sizeof i->manufacturerID, "teste");
    i->flags = CKF_TOKEN_PRESENT | CKF_HW_SLOT;
    return CKR_OK;
}

static CK_RV token(CK_SLOT_ID s, CK_TOKEN_INFO_PTR i)
{
    (void)s;
    int fd = compartilha() ? trava() : -1;
    memset(i, 0, sizeof *i);
    preencher(i->label, sizeof i->label, "ESPERA");
    preencher(i->manufacturerID, sizeof i->manufacturerID, "teste");
    preencher(i->model, sizeof i->model, "espera");
    preencher(i->serialNumber, sizeof i->serialNumber, "1");
    i->flags = CKF_TOKEN_INITIALIZED | CKF_LOGIN_REQUIRED | CKF_PROTECTED_AUTHENTICATION_PATH;
    soltar(fd);
    return CKR_OK;
}

static CK_RV abrir(CK_SLOT_ID s, CK_FLAGS f, CK_VOID_PTR a, CK_NOTIFY n, CK_SESSION_HANDLE_PTR h)
{
    (void)s; (void)f; (void)a; (void)n;
    *h = 1;
    return CKR_OK;
}

static CK_RV fechar(CK_SESSION_HANDLE h) { (void)h; return CKR_OK; }

static CK_RV entrar(CK_SESSION_HANDLE h, CK_USER_TYPE u, CK_UTF8CHAR_PTR pin, CK_ULONG n)
{
    (void)h; (void)u; (void)pin; (void)n;
    const char *s = getenv("ESPERA_SEGUNDOS");
    int fd = trava();
    sleep(s ? (unsigned)atoi(s) : 25);
    soltar(fd);
    return CKR_OK;
}

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
    tabela.C_OpenSession = abrir;
    tabela.C_CloseSession = fechar;
    tabela.C_Login = entrar;
    *saida = &tabela;
    return CKR_OK;
}
