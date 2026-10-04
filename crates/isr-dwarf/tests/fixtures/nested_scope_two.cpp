#include "nested_scope_shared.h"
struct UsesNested {
    Lazy::Detail detail;
    Dup dup;
    Dup::Hidden hidden;
    Alpha alpha;
    Alpha::ExpansionData alpha_data;
    Beta beta;
    Beta::ExpansionData beta_data;
    ns::Scoped scoped;
};
UsesNested two_var;
