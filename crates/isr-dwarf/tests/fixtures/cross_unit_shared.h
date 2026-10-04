struct Shared {
    struct Inner {
        int a;
        long b;
    } inner;
    union {
        int i;
        float f;
    };
    Shared *next;
    static int instances;
    int get() const { return inner.a; }
};
