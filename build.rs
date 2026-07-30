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

  let target = env::var("TARGET").unwrap();
  let profile = env::var("PROFILE").unwrap();

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
    .define("LIBXML2_WITH_THREADS", "ON");

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
  } else {
    build.flag("-fPIC");
  }

  build.compile("schema_refs");
}
