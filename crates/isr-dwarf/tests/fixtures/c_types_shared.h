/* Anonymous struct named by a typedef, like `mach_msg_header_t` in XNU. */
typedef struct {
    unsigned int bits;
    unsigned int size;
    void *remote;
} header_t;

/* Anonymous enum named by a typedef, like XNU `__options_decl`. */
typedef enum : unsigned int {
    FLAG_NONE = 0,
    FLAG_WAIT = 1,
} flags_t;

/* Anonymous members, like the union inside `struct ipc_kmsg` in XNU. */
struct kmsg {
    struct kmsg *next;
    union {
        void *big_data;
        struct {
            header_t *kdata;
            void *udata;
        };
    };
    unsigned int aux_size;
};

int add(int left, int right);
int sum(int count, ...);
