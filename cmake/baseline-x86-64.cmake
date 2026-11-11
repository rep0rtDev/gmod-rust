# Toolchain file for the native C++ dependencies built through the `cmake` crate (Jolt via
# joltc-sys): Jolt's CMake turns on AVX2 / FMA / F16C / LZCNT / TZCNT by default, which makes
# the binaries die with an illegal instruction (STATUS_ILLEGAL_INSTRUCTION, 0xC000001D) on
# CPUs older than Haswell / Zen before a single line is printed. Keep the baseline x86-64 plus
# SSE4.2, like the game itself. Selected from .cargo/config.toml (`CMAKE_TOOLCHAIN_FILE_*`).
set(USE_SSE4_1 ON CACHE BOOL "" FORCE)
set(USE_SSE4_2 ON CACHE BOOL "" FORCE)
set(USE_AVX OFF CACHE BOOL "" FORCE)
set(USE_AVX2 OFF CACHE BOOL "" FORCE)
set(USE_AVX512 OFF CACHE BOOL "" FORCE)
set(USE_LZCNT OFF CACHE BOOL "" FORCE)
set(USE_TZCNT OFF CACHE BOOL "" FORCE)
set(USE_F16C OFF CACHE BOOL "" FORCE)
set(USE_FMADD OFF CACHE BOOL "" FORCE)
