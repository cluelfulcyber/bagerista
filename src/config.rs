//! Start-up settings, read from environment variables (all optional).
//!
//! | Variable                   | Default            |
//! |----------------------------|--------------------|
//! | `BAGERISTA_DB`             | `./bagerista.db`   |
//! | `BAGERISTA_ADDR`           | `127.0.0.1:8080`   |
//! | `BAGERISTA_SEED`           | `1` (demo data on) |
//! | `BAGERISTA_SECURE_COOKIES` | `0` (`1` behind HTTPS) |
//! | `BAGERISTA_OPEN_BROWSER`   | `0`                |

#[derive(Debug, Clone)]
pub struct Config {
    /// Path of the SQLite file; created if it does not exist.
    pub db_path: String,
    /// Address the web server listens on.
    pub addr: String,
    /// Mark the session cookie `Secure` (only works over HTTPS).
    pub secure_cookies: bool,
    /// Fill an empty database with demo data.
    pub seed: bool,
    /// Open the site in the default browser after start-up.
    pub open_browser: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            db_path: "./bagerista.db".to_string(),
            addr: "127.0.0.1:8080".to_string(),
            secure_cookies: false,
            seed: true,
            open_browser: false,
        }
    }
}

impl Config {
    pub fn from_env() -> Config {
        let defaults = Config::default();
        Config {
            db_path: env_string("BAGERISTA_DB").unwrap_or(defaults.db_path),
            addr: env_string("BAGERISTA_ADDR").unwrap_or(defaults.addr),
            secure_cookies: env_flag("BAGERISTA_SECURE_COOKIES").unwrap_or(defaults.secure_cookies),
            seed: env_flag("BAGERISTA_SEED").unwrap_or(defaults.seed),
            open_browser: env_flag("BAGERISTA_OPEN_BROWSER").unwrap_or(defaults.open_browser),
        }
    }
}

fn env_string(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// "1", "true", "yes" → true; "0", "false", "no" → false; anything else → default.
fn env_flag(name: &str) -> Option<bool> {
    match env_string(name)?.trim().to_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}
