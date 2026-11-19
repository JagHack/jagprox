//! `jagprox login --email`: email + password for terminals without a browser.

use std::io::{BufRead, Write};

use jagprox_core::api::Backend;

pub async fn email_login(email: Option<String>) -> Result<String, String> {
    let email = match email {
        Some(e) => e,
        None => {
            print!("Email: ");
            let _ = std::io::stdout().flush();
            let mut line = String::new();
            std::io::stdin()
                .lock()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?;
            line.trim().to_string()
        }
    };
    if email.is_empty() {
        return Err("no email given".into());
    }
    let password = tokio::task::spawn_blocking(|| rpassword::prompt_password("Password: "))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("could not read password: {e}"))?;
    Backend::cli_login(&email, &password)
        .await
        .map_err(|e| e.to_string())
}
