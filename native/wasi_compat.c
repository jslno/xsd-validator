/*
 * Rust's bundled wasm32-wasip1 crt1-reactor.o (stable before the wasi-sdk 34
 * libc update) calls __wasi_init_tp, which wasi-sdk 34's libc no longer
 * defines because it initializes the thread pointer statically. Its own
 * crt1-reactor.o makes no such call, so a no-op matches it. Weak, so a libc
 * that still provides the symbol wins.
 */
__attribute__((weak)) void __wasi_init_tp(void) {}
