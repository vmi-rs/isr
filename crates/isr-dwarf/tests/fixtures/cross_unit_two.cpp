#include "cross_unit_shared.h"
struct TwoOnly {
    Shared s;
    Shared::Inner inner;
    long y;
};
TwoOnly two_var;
