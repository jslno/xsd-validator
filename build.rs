use std::env;
use std::path::PathBuf;

fn main() {
  napi_build::setup();

  let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
  let libxml2_src = manifest_dir.join("vendor").join("libxml2");

  if !libxml2_src.join("CMakeLists.txt").is_file() {
    panic!(
      "vendored libxml2 not found at {}. Run: git submodule update --init --recursive",
      libxml2_src.display()
    );
  }

  println!(
    "cargo:rerun-if-changed={}",
    libxml2_src.join("CMakeLists.txt").display()
  );
  println!(
    "cargo:rerun-if-changed={}",
    libxml2_src.join("VERSION").display()
  );
  println!(
    "cargo:rerun-if-changed={}",
    manifest_dir.join("native").join("schema_refs.c").display()
  );
  println!(
    "cargo:rerun-if-changed={}",
    manifest_dir.join("native").join("wasi_compat.c").display()
  );

  let target = env::var("TARGET").unwrap();
  let profile = env::var("PROFILE").unwrap();
  let is_wasm = target.starts_with("wasm32");

  let mut config = cmake::Config::new(&libxml2_src);
  config
    .define("BUILD_SHARED_LIBS", "OFF")
    .define("LIBXML2_WITH_PYTHON", "OFF")
    .define("LIBXML2_WITH_ICONV", "OFF")
    .define("LIBXML2_WITH_ICU", "OFF")
    .define("LIBXML2_WITH_ZLIB", "OFF")
    .define("LIBXML2_WITH_LZMA", "OFF")
    .define("LIBXML2_WITH_HTTP", "OFF")
    .define("LIBXML2_WITH_MODULES", "OFF")
    .define("LIBXML2_WITH_CATALOG", "OFF")
    .define("LIBXML2_WITH_PROGRAMS", "OFF")
    .define("LIBXML2_WITH_TESTS", "OFF")
    .define("LIBXML2_WITH_DOCS", "OFF")
    .define("LIBXML2_WITH_DEBUG", "OFF")
    .define("LIBXML2_WITH_SCHEMAS", "ON")
    .define("LIBXML2_WITH_THREADS", if is_wasm { "OFF" } else { "ON" });

  if is_wasm {
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    let wasi_sdk = env::var("WASI_SDK_PATH")
      .map(PathBuf::from)
      .unwrap_or_else(|_| {
        panic!("WASI_SDK_PATH must point to a wasi-sdk install to build libxml2 for {target}")
      });
    let toolchain = ["wasi-sdk-p1.cmake", "wasi-sdk.cmake"]
      .iter()
      .map(|name| wasi_sdk.join("share").join("cmake").join(name))
      .find(|path| path.is_file())
      .unwrap_or_else(|| {
        panic!(
          "no wasi-sdk CMake toolchain file under {}",
          wasi_sdk.display()
        )
      });

    config
      .define("CMAKE_TOOLCHAIN_FILE", &toolchain)
      .define("WASI_SDK_PREFIX", &wasi_sdk)
      .define("CMAKE_SYSTEM_NAME", "WASI");

    // napi-build reserves a 64 MB stack, half of a Cloudflare Worker's 128 MB
    // isolate; the last -zstack-size wins. Keep napi.wasm.initialMemory above
    // stack + static data.
    println!("cargo:rustc-link-arg=-zstack-size=8388608");
  }

  // Match the MSVC CRT used by Node.js (/MD).
  if target.contains("msvc") {
    config.static_crt(false);
    if profile == "debug" {
      config.define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDebugDLL");
    } else {
      config.define("CMAKE_MSVC_RUNTIME_LIBRARY", "MultiThreadedDLL");
    }
  }

  let dst = config.build();

  let include_dir = dst.join("include").join("libxml2");
  let lib_dir = dst.join("lib");
  println!("cargo:rustc-link-search=native={}", lib_dir.display());

  if target.contains("msvc") {
    let lib_name = if profile == "debug" {
      "libxml2sd"
    } else {
      "libxml2s"
    };
    println!("cargo:rustc-link-lib=static={}", lib_name);
    println!("cargo:rustc-link-lib=bcrypt");
  } else if is_wasm {
    // wasi-libc (including libm) comes from the wasi-sdk sysroot via wasm-ld.
    println!("cargo:rustc-link-lib=static=xml2");
  } else {
    println!("cargo:rustc-link-lib=static=xml2");
    println!("cargo:rustc-link-lib=m");
    if target.contains("linux") || target.contains("android") || target.contains("freebsd") {
      println!("cargo:rustc-link-lib=pthread");
    }
  }

  // Helper that walks schema documents using libxml2's native xmlNode layout.
  let mut build = cc::Build::new();
  build
    .file(manifest_dir.join("native").join("schema_refs.c"))
    .include(&include_dir)
    .define("LIBXML_STATIC", None)
    .warnings(false);

  if target.contains("msvc") {
    if profile == "debug" {
      build.flag("/MDd");
    } else {
      build.flag("/MD");
    }
  } else if is_wasm {
    build.file(manifest_dir.join("native").join("wasi_compat.c"));
  } else {
    build.flag("-fPIC");
  }

  build.compile("schema_refs");
}
