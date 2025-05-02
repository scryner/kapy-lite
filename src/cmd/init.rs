use anyhow::{Result, anyhow};
use console::style;
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::config;

pub async fn do_init(force: bool) -> Result<()> {
    println!("Initializing kapylite...");

    // get default config_path
    let default_path = config::default_path();

    let app_home = default_path.app_home();
    let conf_path = default_path.config_path();

    // check configuration file is already existed
    if fs::metadata(conf_path).await.is_ok() && !force {
        return Err(anyhow!(
            "Already initialized, config is on '{:?}'",
            conf_path
        ));
    }

    // create kapy home directory
    print!(
        "\tCreating kapylite home directory '{:?}'...",
        app_home.as_os_str()
    );
    match fs::create_dir(app_home).await {
        Ok(()) => println!("\t{}", style("[  OK  ]").green()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AlreadyExists {
                println!("\t{}", style("[  OK  ]").green())
            } else {
                println!("\t{}", style("[FAILED]").red());
                return Err(anyhow!("Failed to create directory: {}", e));
            }
        }
    }

    // make default configuration to the directory
    print!("\tCreating configurations on '{:?}'...", conf_path);
    match fs::File::create(conf_path).await {
        Ok(mut file) => match file.write_all(DEFAULT_CONF_YAML.as_bytes()).await {
            Ok(_) => println!("\t{}", style("[  OK  ]").green()),
            Err(e) => {
                println!("\t{}", style("[FAILED]").red());
                return Err(anyhow!("Failed to write configuration to file: {}", e));
            }
        },
        Err(e) => {
            println!("\t{}", style("[FAILED]").red());
            return Err(anyhow!("Failed to create file: {}", e));
        }
    }

    println!(
        "\nYou must edit configurations on '{}'",
        style(conf_path.to_str().unwrap()).cyan()
    );

    Ok(())
}

const DEFAULT_CONF_YAML: &str = r#"import:
  from: YOUR_ORIGIN_PATH
  to: YOUR_TARGET_PATH
"#;
