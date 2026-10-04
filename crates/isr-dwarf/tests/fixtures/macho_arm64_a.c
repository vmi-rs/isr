static int counter = 1;
int _leading = 3;
int *counter_a(void) { return &counter; }
int main(void) { return *counter_a(); }
