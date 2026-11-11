# Cross build of the C++ dependencies for Windows with llvm-mingw (see README): the compilers
# the `cmake` crate would otherwise pass itself, plus the baseline x86-64 ISA options. No
# CMAKE_RC_COMPILER here: the crate passes its own on the command line, and a toolchain file
# disagreeing with it makes CMake wipe the cache and re-run, losing the FORCEd options above.
set(CMAKE_SYSTEM_NAME Windows)
set(CMAKE_SYSTEM_PROCESSOR AMD64)
set(CMAKE_C_COMPILER x86_64-w64-mingw32-clang)
set(CMAKE_CXX_COMPILER x86_64-w64-mingw32-clang++)
set(CMAKE_AR x86_64-w64-mingw32-ar)
set(CMAKE_RANLIB x86_64-w64-mingw32-ranlib)
set(CMAKE_FIND_ROOT_PATH_MODE_PROGRAM NEVER)
include(${CMAKE_CURRENT_LIST_DIR}/baseline-x86-64.cmake)
