struct Lazy {
    virtual ~Lazy();
    struct Detail {
        int d1;
        int d2;
    };
    Detail *detail;
};

struct Dup {
    struct Hidden {
        long h;
    };
    int x;
};

struct Alpha {
    struct ExpansionData {
        int a;
    } *reserved;
};

struct Beta {
    struct ExpansionData {
        long x;
        long y;
    } *reserved;
};

namespace ns {
struct Scoped {
    struct Leaf {
        short v;
    } leaf;
};
}
