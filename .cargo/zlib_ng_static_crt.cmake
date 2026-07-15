# CMake toolchain shim for the zlib-ng (libz-ng-sys) build inside the ModBox21
# native umbrella, wired in via CMAKE_TOOLCHAIN_FILE in ../.cargo/config.toml.
#
# Rust always links the *non-debug* static MSVC CRT (libcmt) under
# `-C target-feature=+crt-static`, in both debug and release cargo profiles.
# zlib-ng's CMakeLists sets `cmake_minimum_required(VERSION 3.14...3.31.0)`,
# which makes CMP0091 NEW: the MSVC runtime is then selected by
# CMAKE_MSVC_RUNTIME_LIBRARY (default = the *DLL* runtime) rather than by any
# /MD or /MT in CMAKE_<LANG>_FLAGS. Without this shim the vendored C objects
# come out /MD(d) (__imp__* imports) and fail to link against the static-CRT
# umbrella (esaxx-rs MT_StaticRelease).
set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreaded" CACHE STRING "" FORCE)

# The Debug config adds /RTC1 (basic runtime checks), which requires a *debug*
# CRT and is rejected when compiling against /MT. Pin the Debug flag sets to
# runtime-check-free, NDEBUG values so a debug cargo build (e.g.
# `cargo test -p bsarchive_native`) still compiles and links against /MT.
set(CMAKE_C_FLAGS_DEBUG   "/Od /Ob0 /DNDEBUG" CACHE STRING "" FORCE)
set(CMAKE_CXX_FLAGS_DEBUG "/Od /Ob0 /DNDEBUG" CACHE STRING "" FORCE)
