extern crate syntex;
extern crate serde_codegen;

fn main() {
    let out_dir = std::env::var_os("OUT_DIR").unwrap();

    let src = std::path::Path::new("src/lib/types.rs.in");
    let dst = std::path::Path::new(&out_dir).join("types.rs");

    serde_codegen::expand(&src, &dst).unwrap();
}
