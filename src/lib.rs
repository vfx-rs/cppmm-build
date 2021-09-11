use regex::Regex;
use std::path::Path;

/// Build a packaged dependency that is stored in directory `name` under
/// `thirdparty` in the project tree, e.g. `thirdparty/zlib`.
///
pub fn build_thirdparty(
    name: &str,
    target_dir: &Path,
    profile: &str,
    definitions: &[(&str, &str)],
) -> String {
    // We need to create a dedicated subdirectory for the build or cmake will
    // wipe it every time, forcing a rebuild
    let out_dir = target_dir.join(&format!("build-{}", name));
    match std::fs::create_dir(&out_dir) {
        Ok(_) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => panic!(
            "Could not create build directory '{}': {}",
            out_dir.display(),
            e
        ),
    }

    let mut config = cmake::Config::new(&format!("thirdparty/{}", name));
    config.profile(profile);
    config.define("CMAKE_INSTALL_PREFIX", target_dir.to_str().unwrap());
    config.define("CMAKE_PREFIX_PATH", target_dir.join("lib").join("cmake"));
    config.out_dir(&out_dir);

    for def in definitions {
        config.define(def.0, def.1);
    }

    config
        .build()
        .to_str()
        .expect(&format!("Unable to convert {} dst to str", name))
        .to_string()
}

/// Path information for a linked library.
///
/// For a `path` '/home/libs/libmylib.so`, `basename` will be `mylib` and `libname`
/// will be `libmylib.so`
///
#[derive(Debug)]
pub struct DylibPathInfo {
    pub path: String,
    pub basename: String,
    pub libname: String,
}

#[derive(Debug)]
pub enum LinkArg {
    LinkDir(String),
    LinkLib(String),
    Path(DylibPathInfo),
}

#[cfg(not(target_os = "windows"))]
fn is_dylib_path(s: &str, re: &Regex) -> Option<LinkArg> {
    if let Ok(_) = std::env::var("CPPMM_DEBUG_BUILD") {
        println!("cargo:warning=- {}", s);
    }
    
    if let Some(pos @ 0) = s.find("-l") {
        return Some(LinkArg::LinkLib(s[2..].to_string()))
    } else if let Some(pos @ 0) = s.find("-L") {
        if let Ok(_) = std::env::var("CPPMM_DEBUG_BUILD") {
            println!("cargo:warning=    is a link dir {}", s);
        }
        return Some(LinkArg::LinkDir(s[2..].to_string()))
    } else if let Some(m) = re.captures_iter(s).next() {
        if let Some(c0) = m.get(0) {
            if let Some(c1) = m.get(1) {
                if let Ok(_) = std::env::var("CPPMM_DEBUG_BUILD") {
                    println!("cargo:warning=    is a dylib path {}", s);
                }
                return Some(LinkArg::Path(DylibPathInfo {
                    path: s.to_string(),
                    basename: c0.as_str().to_string(),
                    libname: c1.as_str().to_string(),
                }));
            }
        }
    }
    if let Ok(_) = std::env::var("CPPMM_DEBUG_BUILD") {
        println!("cargo:warning=    is not a dylib path");
    }

    None
}

#[cfg(target_os = "windows")]
fn is_dll_lib_path(s: &str, re: &Regex) -> Option<LinkArg> {
    if let Some(m) = re.captures_iter(s).next() {
        if let Some(c0) = m.get(0) {
            if let Some(c1) = m.get(1) {
                return Some(LinkArg::Path(DylibPathInfo {
                    path: s.to_string(),
                    basename: c0.as_str().to_string(),
                    libname: c1.as_str().to_string(),
                }));
            }
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn get_linking_from_vsproj(
    build_path: &Path,
    clib_versioned_name: &str,
    build_type: &str,
) -> Option<Vec<LinkArg>> {
    use quick_xml::events::{BytesEnd, BytesStart, Event};
    use quick_xml::Reader;
    use std::borrow::Borrow;
    use std::io::Cursor;
    use std::iter;

    let proj_path = build_path.join(format!("{}.vcxproj", clib_versioned_name));
    let proj_xml = std::fs::read_to_string(&proj_path).ok()?;

    let re = Regex::new(r"(?:.*\\(.*))(\.lib)$").unwrap();

    let mut reader = Reader::from_str(&proj_xml);
    reader.trim_text(true);

    let mut in_item_definition = false;
    let mut in_link = false;
    let mut in_deps = false;

    let mut buf = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => match e.name() {
                b"ItemDefinitionGroup" => {
                    for attr in e.attributes() {
                        if let Ok(attr) = attr {
                            if attr.key == b"Condition" {
                                let s =
                                    std::str::from_utf8(attr.value.borrow())
                                        .unwrap();
                                if s.contains(build_type) {
                                    in_item_definition = true;
                                }
                            }
                        }
                    }
                }
                b"Link" if in_item_definition => {
                    in_link = true;
                }
                b"AdditionalDependencies" if in_item_definition && in_link => {
                    in_deps = true;
                }
                _ => (),
            },
            Ok(Event::End(ref e)) => match e.name() {
                b"ItemDefinitionGroup" => {
                    in_item_definition = false;
                }
                b"Link" => {
                    in_link = false;
                }
                b"AdditionalDependencies" => in_deps = false,
                _ => (),
            },
            Ok(Event::Text(e)) if in_deps => {
                let mut dlls = Vec::new();
                for tok in e.unescape_and_decode(&reader).unwrap().split(";") {
                    if let Some(dll) = is_dll_lib_path(tok, &re) {
                        dlls.push(dll)
                    }
                }
                return Some(dlls);
            }
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error parsing vsproj xml"),
            _ => (),
        }
    }

    None
}

#[cfg(target_os = "windows")]
fn get_linking_from_nmake(
    build_path: &Path,
    clib_versioned_name: &str,
) -> Option<Vec<LinkArg>> {
    let build_make_path = build_path
        .join("CMakeFiles")
        .join(format!("{}-shared.dir", clib_versioned_name))
        .join("build.make");

    let build_make = std::fs::read_to_string(&build_make_path).ok()?;

    let re = Regex::new(r"(?:.*\\(.*))(\.lib)$").unwrap();

    let mut found_slash_dll = false;
    let mut libs = Vec::new();
    // println!("cargo:warning=Found links:");
    for tok in build_make.split_whitespace() {
        if tok == "/dll" {
            found_slash_dll = true;
        } else if found_slash_dll {
            if tok == "<<" {
                break;
            } else {
                if let Some(dlp) = is_dll_lib_path(tok, &re) {
                    libs.push(dlp);
                }
            }
        }
    }

    Some(libs)
}

#[cfg(target_os = "windows")]
/// Parse the generated project files from our C wrapper in order to get its 
/// set of linker arguments.
///
/// On Unices this will parse CMake's auxiliary link.txt file for `.so`s or 
/// `.dylib`s. On Windows this will parse NMake or VS XML project files.
///
pub fn get_linking_from_cmake(
    build_path: &Path,
    clib_versioned_name: &str,
    build_type: &str,
) -> Vec<LinkArg> {
    if let Some(libs) =
        get_linking_from_vsproj(build_path, clib_versioned_name, build_type)
    {
        libs
    } else if let Some(libs) =
        get_linking_from_nmake(build_path, clib_versioned_name)
    {
        libs
    } else {
        panic!("Could not open either vsproj or nmake build");
    }
}

#[cfg(not(target_os = "windows"))]
pub fn get_linking_from_cmake(
    build_path: &Path,
    clib_versioned_name: &str,
    _build_type: &str,
) -> Vec<LinkArg> {
    let link_txt_path = build_path
        .join("CMakeFiles")
        .join(format!("{}.dir", clib_versioned_name))
        .join("link.txt");
    let link_txt = std::fs::read_to_string(&link_txt_path).expect(&format!(
        "Could not read link_txt_path: {}",
        link_txt_path.display()
    ));

    if let Ok(_) = std::env::var("CPPMM_DEBUG_BUILD") {
        println!("cargo:warning=Reading link.txt {}", link_txt);
    }

    let re = Regex::new(
        r"lib([^/]+?)(?:\.dylib|\.so|\.so.\d+|\.so.\d+.\d+|\.so.\d+.\d+.\d+)$",
    )
    .unwrap();

    // Try and figure out what are libraries we want to copy to target.
    // Libraries will end with `.so` or `.so.28.1.0` or `.dylib`

    // First, strip off everything up to and including the initial "-o whatever.so"
    let mut link_txt = link_txt.split(' ');
    while let Some(s) = link_txt.next() {
        if s == "-o" {
            // pop off the output lib as well
            let _ = link_txt.next();
            break;
        }
    }

    // Now match all the remaining arguments against a regex looking for
    // shared library paths.
    link_txt.filter_map(|s| is_dylib_path(s, &re)).collect()
}

pub struct Dependency {
    pub name: &'static str,
    pub definitions: Vec<(&'static str, &'static str)>,
}

use std::fmt;
impl fmt::Debug for Dependency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

/// Build a standard-formatted cppmm c wrapper project and its dependencies.
///
/// If the environment variable `CMAKE_PREFIX_PATH` is set, any `dependencies`
/// will be assumed to be present on the system, available in `CMAKE_PREFIX_PATH`.
/// If `CMAKE_PREFIX_PATH` is not set, the list of dependencies will be built
/// from the `thirdparty` directory.
///
/// `project_name` controls the name of the generated C library, as well as the 
/// names of environment variables the user can set to control the build. For
/// example, setting `project_name` to `openexr` will cause the script to respond
/// to:
/// * `CPPMM_OPENEXR_BUILD_LIBRARIES` - Ignore `CMAKE_PREFIX_PATH` and force  
/// building the dependencies if this is set to "1".
/// * `CPPMM_OPENEXR_BUILD_TYPE` - Set the build profile used for the C library 
/// and all dependencies. This defaults to "Release" so you can use this to set 
/// it to "Debug", for example.
///
/// `major_version` and `minor_version` are the crate version numbers and are 
/// baked into the C library filename.
///
pub fn build(project_name: &str, major_version: u32, minor_version: u32, dependencies: &[Dependency]) {

    let env_build_libraries = format!("CPPMM_{}_BUILD_LIBRARIES", project_name.to_ascii_uppercase());
    let env_build_type = format!("CPPMM_{}_BUILD_TYPE", project_name.to_ascii_uppercase());

    // If the user has set CMAKE_PREFIX_PATH then we don't want to build the
    // bundled libraries, *unless* they have also set CPPMM_<project_name>_BUILD_LIBRARIES=1
    let build_libraries = if std::env::var("CMAKE_PREFIX_PATH").is_ok() {
        if let Ok(obl) = std::env::var(&env_build_libraries) {
            obl == "1"
        } else {
            false
        }
    } else {
        true
    };

    let out_dir = std::env::var("OUT_DIR").unwrap();
    let target_dir = Path::new(&out_dir).ancestors().skip(3).next().unwrap();

    let clib_name = format!("{}-c", project_name);
    let clib_versioned_name =
        format!("{}-c-{}_{}", project_name, major_version, minor_version);
    let clib_shared_versioned_name =
        format!("{}-c-{}_{}-shared", project_name, major_version, minor_version);

    let lib_path = target_dir.join("lib");
    let bin_path = target_dir.join("bin");
    let cmake_prefix_path = lib_path.join("cmake");

    // allow user to override build type with environment variables
    let build_type =
        if let Ok(build_type) = std::env::var(&env_build_type) {
            build_type
        } else {
            "Release".to_string()
        };

    let dst = if build_libraries {
        println!("cargo:warning=Building packaged dependencies {:?}", dependencies);
        for dep in dependencies {
            build_thirdparty(dep.name, target_dir, &build_type, &dep.definitions);
        }

        cmake::Config::new(clib_name)
            .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON")
            .define("CMAKE_PREFIX_PATH", cmake_prefix_path.to_str().unwrap())
            .profile(&build_type)
            .build()
    } else {
        println!("cargo:warning=Using system dependencies {:?}", dependencies);
        cmake::Config::new(clib_name)
            .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON")
            .profile(&build_type)
            .build()
    };

    let build_path = Path::new(&dst).join("build");

    let link_args = get_linking_from_cmake(
        &build_path,
        &clib_shared_versioned_name,
        &build_type,
    );
    println!("cargo:warning=Link libs: {:?}", link_args);

    // Link our wrapper library
    //
    // We currently build a dylib on windows just so we can enable Debug
    // builds. This is because Rust always links against the release msvcrt 
    // (presumably since the debug one is unusable in a lot of situations), thus
    // we cannot link statically since setting the C shim to Debug mode will 
    // cause it to link against the debug msvcrt. This in turn causes all sorts
    // of bad shit to happen (segfaults mostly). By the way, did you know that 
    // STL types are different sizes in debug and release builds on Windows?
    // I didn't until today because I couldn't imagine a world in which something
    // like that would be allowed to happen.
    //
    // In theory, you can override this, but like most things with CMake, the 
    // correct incantations are buried somewhere in vague mailing list 
    // threads, and don't actually seem to work (at least not with VS generators, 
    // which appear to want to force the runtime for you).
    //
    // So, the easiest way out here is just to build everything from the C shim
    // down as a DLL so we can neatly sidestep all this (because the C library 
    // provides a nice ABI dambreak against the insanity).
    //
    // We still build statically on Linux since that way you don't need to install
    // the DSO along with any Rust binaries you might want to build. Ultimately
    // installation in a production environment will require a bit more thought,
    // but suffice to say it's complex. On Windows at least, just copying DLLs
    // around everywhere seems to be the norm so we assume it's not the end of 
    // the world.
    //
    println!("cargo:rustc-link-search=native={}", dst.display());
    #[cfg(not(target_os = "windows"))]
    println!("cargo:rustc-link-lib=static={}", clib_versioned_name);
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-lib=dylib={}", clib_shared_versioned_name);

    if build_libraries {
        // Link against the stuff what we built
        println!("cargo:rustc-link-search=native={}", lib_path.display());
        // we don't actually want to link against anything in /bin but we 
        // need to tell rustc where the DLLs are on windows and this is the 
        // way to do it
        println!("cargo:rustc-link-search=native={}", bin_path.display());
    }

    for arg in link_args {
        // Link against all our dependencies
        match arg {
            LinkArg::Path(d) => {
                let libdir = Path::new(&d.path).parent().unwrap();
                println!("cargo:rustc-link-search=native={}", libdir.display());
                println!("cargo:rustc-link-lib=dylib={}", &d.libname);
            }
            LinkArg::LinkDir(dir) => {
                println!("cargo:rustc-link-search=native={}", dir);
            }
            LinkArg::LinkLib(lib) => {
                println!("cargo:rustc-link-lib=dylib={}", lib);
            }
        }
    }

    // On unices we need to link against the stdlib
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-lib=dylib=stdc++");
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-lib=dylib=c++");

    // Insert the C++ ABI info
    //
    // abigen is a small binary that's autogenerated by cppmm. It simply outputs
    // the size of all opaquebytes types to a file, `abigen.txt`. Meanwhile, 
    // cppmm sets up both the C and Rust layer source with placeholder markers
    // that are replaced by the Python script `insert_abi.py`, below. 
    //
    // We do this because certain types (STL mainly) are different sizes between
    // platforms (and even between build types on Windows!), and generating 
    // their ABI info at build time here saves us from having to run the entire
    // binding generation at the crate build level, and thus keeps a libclang 
    // dependency out of all our end-user crates.
    //
    let build_dir = Path::new(&out_dir).join("build");
    let abigen_bin = build_dir.join("abigen").join("abigen");
    let abigen_txt = build_dir.join("abigen.txt");

    // Run abigen again if the output doesn't exist.
    if !abigen_txt.exists() {
        let _ = std::process::Command::new(abigen_bin)
            .current_dir(build_dir)
            .output()
            .expect("Could not run abigen");
    }

    let cppmm_abi_out = Path::new(&out_dir).join("cppmm_abi_out").join("cppmmabi.rs");

    // if the generated rust doesn't exist, run the python to generate it
    if !cppmm_abi_out.exists() {
        let output = std::process::Command::new("python")
            .args(&[&format!("{}-c/abigen/insert_abi.py", project_name), 
                "cppmm_abi_in", 
                &format!("{}/cppmm_abi_out", out_dir), 
                &format!("{}/build/abigen.txt", out_dir)])
            .output()
            .expect("Could not launch python insert_abi.py");

        if !output.status.success() {{
            for line in std::str::from_utf8(&output.stderr).unwrap().lines() {{
                println!("cargo:warning={}", line);
            }}
            panic!("python insert_abi failed");
        }}
    }

}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
