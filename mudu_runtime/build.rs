fn main() {
    println!("cargo:rerun-if-changed=wit/kernel.wit");
}