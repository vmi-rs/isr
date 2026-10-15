#include "c_types_shared.h"

/* A named struct and an anonymous struct named by a typedef share a name. */
struct taken {
    int named;
};
typedef struct {
    long anonymous;
} taken;

/* The same typedef name names a different anonymous struct in the other unit. */
typedef struct {
    int first;
} conflict_t;

struct kmsg kmsg_one;
header_t header_one;
flags_t flags_one;
struct taken taken_named;
taken taken_anonymous;
conflict_t conflict_one;

int add(int left, int right) { return left + right; }

int sum(int count, ...) { return count; }

static int hidden(int value) { return value + 1; }

int square(int value) { return value * value; }

flags_t pick(struct kmsg *kmsg, header_t *header)
{
    return (flags_t) square(hidden((int) kmsg->aux_size + (int) header->size));
}
