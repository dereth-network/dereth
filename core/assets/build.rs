//! Tell the crate's tests whether they were compiled without optimisation.
//!
//! Some bounds are statements about an optimised build (a stack budget, say, that an unoptimised
//! build exceeds because it keeps every callee's frame). Whether debug assertions are on does not
//! answer that: the test tiers run optimised with debug assertions on. So the opt level is read
//! here, and `cfg(unoptimised)` is set when it is 0.

fn main() {
    println!("cargo::rustc-check-cfg=cfg(unoptimised)");
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("OPT_LEVEL").is_ok_and(|level| level == "0") {
        println!("cargo::rustc-cfg=unoptimised");
    }
}
