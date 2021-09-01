# cppmm-build
Build script utlities for binding projects created with cppmm

# Example build.rs

When placed in the `build.rs` for `openexr-sys` this will build the cppmm-generated C wrapper libraries, including filling in platform-specific ABI information.

Packaged dependencies are assumed to live in `thirdparty/<dependency>` and the listed ones will be built and linked against. Users wishing to build against
system libraries should set the `CMAKE_PREFIX_PATH` environment variable. If the `CMAKE_PREFIX_PATH` environment variable is set, but you wish to build the
pacakged depdencies anyway, set `CPPMM_OPENEXR_BUILD_LIBRARIES=1`. If you wish to control the CMake build type, set e.g. `CPPMM_OPENEXR_BUILD_TYPE=Debug` 
(default is "Release").

```rust
use cppmm_build::{build, Dependency};

fn main() {
    build(
        "openexr",
        0,
        10,
        &vec![
            Dependency {
                name: "zlib",
                definitions: vec![],
            },
            Dependency {
                name: "Imath",
                definitions: vec![
                    ("IMATH_IS_SUBPROJECT", "ON"),
                    ("BUILD_TESTING", "OFF"),
                    ("BUILD_SHARED_LIBS", "ON"),
                ],
            },
            Dependency {
                name: "openexr",
                definitions: vec![
                    ("OPENEXR_IS_SUBPROJECT", "ON"),
                    ("BUILD_TESTING", "OFF"),
                    ("OPENEXR_INSTALL_EXAMPLES", "OFF"),
                    ("BUILD_SHARED_LIBS", "ON"),
                ],
            },
        ],
    );
}

```
