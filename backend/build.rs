// `sqlx::migrate!` embeds the migrations at compile time, but Cargo only
// rebuilds when Rust sources change. Without this a new migration file alone
// leaves the old binary in place, and the server starts without it.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
