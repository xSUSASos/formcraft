//! Builds OpenCascade (via `occt-sys`) and the C++ half of the bridge.

/// OCCT toolkits in dependency order (dependents first) for single-pass static linkers.
const OCCT_LIBS: &[&str] = &[
    "TKDESTEP",
    "TKDEIGES",
    "TKDESTL",
    "TKDE",
    "TKXCAF",
    "TKCAF",
    "TKLCAF",
    "TKCDF",
    "TKXSBase",
    "TKMesh",
    "TKOffset",
    "TKFeat",
    "TKFillet",
    "TKBool",
    "TKBO",
    "TKShHealing",
    "TKPrim",
    "TKTopAlgo",
    "TKGeomAlgo",
    "TKBRep",
    "TKGeomBase",
    "TKG3d",
    "TKG2d",
    "TKMath",
    "TKernel",
];

fn main() {
    occt_sys::build_occt();
    let occt = occt_sys::occt_path();
    let include = occt.join("include");
    let lib = occt.join("lib");

    let mut b = cxx_build::bridge("src/ffi.rs");
    b.file("cpp/bridge.cxx").std("c++17").include(&include).define("_USE_MATH_DEFINES", None);
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        b.flag("/EHsc").flag("/bigobj").flag("/utf-8");
    } else {
        b.flag_if_supported("-Wno-deprecated-declarations");
    }
    b.compile("formcraft-occt-bridge");

    println!("cargo:rustc-link-search=native={}", lib.display());
    for l in OCCT_LIBS {
        println!("cargo:rustc-link-lib=static={l}");
    }
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("windows") => {
            for l in ["advapi32", "user32", "gdi32"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
        Ok("linux") => {
            for l in ["pthread", "dl"] {
                println!("cargo:rustc-link-lib=dylib={l}");
            }
        }
        _ => {}
    }
    println!("cargo:rerun-if-changed=src/ffi.rs");
    println!("cargo:rerun-if-changed=cpp/bridge.cxx");
    println!("cargo:rerun-if-changed=cpp/bridge.hxx");
}
