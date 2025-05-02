use std::path::Path;

use anyhow::{Result, anyhow};
use console::style;
use tokio::fs;

pub async fn do_clean(cred_path: &Path) -> Result<()> {
    println!("Cleaning kapy...");

    // try to remove credentials
    print!("\tRemoving credentials...");
    match fs::remove_file(&cred_path).await {
        Ok(_) => println!("\t{}", style("[  OK  ]").green()),
        Err(e) => {
            println!("\t{}", style("[FAILED]").red());
            return Err(anyhow!("Failed to remove credentials: {}", e));
        }
    }

    Ok(())
}
