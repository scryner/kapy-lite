use std::env;

fn main() {
    let default_client_id = env::var("DEFAULT_CLIENT_ID").unwrap_or("".to_string());
    let default_client_secret = env::var("DEFAULT_CLIENT_SECRET").unwrap_or("".to_string());

    println!("cargo::rustc-env=DEFAULT_CLIENT_ID={}", default_client_id);
    println!(
        "cargo::rustc-env=DEFAULT_CLIENT_SECRET={}",
        default_client_secret
    );
}
