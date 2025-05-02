use std::fs;
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::path::Path;
use base64::{{Engine as _, engine::general_purpose}};
use anyhow::Result;
use crate::drive::auth::Token;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

pub struct FileCredentials;

impl FileCredentials {
    pub fn marshal(token: &Token) -> Result<String> {
        let json = serde_json::to_string(token)?;

        // base64 encoding
        Ok(general_purpose::STANDARD_NO_PAD.encode(json.as_bytes()))
    }

    pub fn unmarshal(input: Vec<u8>) -> Result<Token> {
        // base64 decoding
        let bytes = general_purpose::STANDARD_NO_PAD.decode(input)?;

        // unmarshal to struct
        let json = String::from_utf8(bytes)?;
        let token = serde_json::from_str::<Token>(&json)?;
        Ok(token)
    }

    pub fn read_file(path: &Path) -> Result<Token> {
        let bytes = fs::read(path)?;
        FileCredentials::unmarshal(bytes)
    }

    pub fn write_file(token: &Token, path: &Path) -> Result<()> {
        let encoded = FileCredentials::marshal(token)?;

        let file;

        #[cfg(unix)]
        {
            file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path);
        }

        #[cfg(windows)]
        {
            file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(path);
        }

        let file = file?;
        let mut writer = BufWriter::new(file);
        Ok(writer.write_all(encoded.as_bytes())?)
    }
}
