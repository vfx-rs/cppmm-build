# cppmm-build
Build script utlities for binding projects created with cppmm

# Example build.rs

When placed in the `build.rs` for `openexr-sys` this will build the cppmm-generated C wrapper libraries, including filling in platform-specific ABI information.

Packaged dependencies are assumed to live in `thirdparty/<dependency>` and the listed ones will be built and linked against. Users wishing to build against
system libraries should set the `CMAKE_PREFIX_PATH` environment variable. 

If the `CMAKE_PREFIX_PATH` environment variable is set, but you wish to build the
pacakged depdencies anyway, set `CPPMM_OPENEXR_BUILD_LIBRARIES=1`. 

If you wish to control the CMake build type, set e.g. `CPPMM_OPENEXR_BUILD_TYPE=Debug` 
(default is "Release").

```rust
use cppmm_build::{build, Dependency};

fn main() {
    build(
        // project name controls the name of the built C libraries as well as the names
        // of the _BUILD_LIBRARIES and _BUILD_TYPE environment variables
        "openexr",
        // project major version
        0,
        // project minor version
        10,
        // list of dependencies that are packaged with the crate and should be built
        // when `CMAKE_PREFIX_PATH` is not set, or `CPPMM_OPENEXR_BUILD_LIBRARIES` is
        // set to 1
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
