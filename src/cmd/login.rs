use std::path::Path;

use anyhow::{Result, anyhow};
use console::style;

use crate::drive::auth::{GoogleAuthenticator, ListenPort};

pub async fn do_login(cred_path: &Path, listen_port: ListenPort) -> Result<()> {
    println!("Login to google drive...");

    // try to login
    print!("\tTrying to login...");
    let auth = GoogleAuthenticator::new(listen_port, cred_path)?;
    match auth.access_token().await {
        Ok(_) => println!("\t{}", style("[  OK  ]").green()),
        Err(e) => {
            println!("\t{}", style("[FAILED]").red());
            return Err(anyhow!("Failed to login: {}", e));
        }
    }

    Ok(())
}
