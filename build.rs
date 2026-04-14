fn main() {
    // Build-script cfg evaluates for the *host*, so gate on the target via
    // CARGO_CFG_TARGET_OS instead. On host `cargo test` this is a no-op.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("espidf") {
        embuild::espidf::sysenv::output();
    }
}
