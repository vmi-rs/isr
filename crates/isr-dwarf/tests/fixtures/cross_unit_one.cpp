#include "cross_unit_shared.h"
int Shared::instances;
struct OneOnly {
    Shared *s;
    int &ref;
};
Shared one_shared;
int one_value;
OneOnly one_var = {&one_shared, one_value};
int one_get() { return one_shared.get(); }
