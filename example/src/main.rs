include!(concat!(env!("OUT_DIR"), "/version.rs"));

fn main() {
    println!("{}", VERSION);
    println!("{}", SOURCES_FINGERPRINT);
    println!("{}", BUILD_FINGERPRINT);
}
