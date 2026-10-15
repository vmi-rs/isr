#include "c_types_export/c_types_shared.h"

typedef struct {
    int second;
} conflict_t;

header_t header_two;
conflict_t conflict_two;

int use_two(header_t *header, conflict_t *conflict)
{
    return add((int) header->size, conflict->second);
}
