//! Owned game-integration configuration (GSI-INTAKE-SPEC §2).

/// First and last candidate loopback ports (inclusive).
pub const PORT_FIRST: u16 = 3000;
pub const PORT_LAST: u16 = 3010;
/// Maximum accepted request body in bytes (GSI-INTAKE-SPEC §3).
pub const BODY_LIMIT_BYTES: usize = 131_072;
/// Requests above this rate are dropped after authentication.
pub const MAX_REQUESTS_PER_SECOND: u32 = 50;

/// Data blocks requested in M1 (data minimization; abilities come later).
pub const M1_DATA_BLOCKS: [&str; 4] = ["provider", "map", "player", "hero"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    InvalidSlug,
    PortOutOfRange,
    InvalidToken,
}

/// Inputs for the owned configuration file.
#[derive(Debug, Clone)]
pub struct GsiConfig<'a> {
    pub display_name: &'a str,
    pub slug: &'a str,
    pub port: u16,
    pub token: &'a str,
    pub data_blocks: &'a [&'a str],
}

impl GsiConfig<'_> {
    /// File name inside `<game>/game/dota/cfg/gamestate_integration/`.
    pub fn file_name(&self) -> Result<String, ConfigError> {
        validate_slug(self.slug)?;
        Ok(format!("gamestate_integration_{}.cfg", self.slug))
    }

    /// Render the key-value configuration text (LF line endings).
    pub fn render(&self) -> Result<String, ConfigError> {
        validate_slug(self.slug)?;
        if !(PORT_FIRST..=PORT_LAST).contains(&self.port) {
            return Err(ConfigError::PortOutOfRange);
        }
        if !valid_token(self.token) {
            return Err(ConfigError::InvalidToken);
        }
        let mut data = String::new();
        for block in self.data_blocks {
            data.push_str(&format!("    \"{block}\" \"1\"\n"));
        }
        Ok(format!(
            "\"{name} intake\"\n{{\n  \"uri\"        \"http://127.0.0.1:{port}/\"\n  \"timeout\"    \"5.0\"\n  \"buffer\"     \"0.1\"\n  \"throttle\"   \"0.1\"\n  \"heartbeat\"  \"1.0\"\n  \"data\"\n  {{\n{data}  }}\n  \"auth\"\n  {{\n    \"token\" \"{token}\"\n  }}\n}}\n",
            name = sanitize_name(self.display_name),
            port = self.port,
            data = data,
            token = self.token,
        ))
    }
}

/// `[a-z0-9_]{3,32}` (GSI-INTAKE-SPEC §2.2).
pub fn validate_slug(slug: &str) -> Result<(), ConfigError> {
    let ok = (3..=32).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(ConfigError::InvalidSlug)
    }
}

/// 43-character base64url (32 random bytes, no padding).
pub fn valid_token(token: &str) -> bool {
    token.len() == 43
        && token
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn sanitize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";

    fn cfg(port: u16) -> GsiConfig<'static> {
        GsiConfig {
            display_name: "Kohbor",
            slug: "kohbor",
            port,
            token: TOKEN,
            data_blocks: &M1_DATA_BLOCKS,
        }
    }

    #[test]
    fn renders_loopback_uri_heartbeat_and_m1_blocks_only() {
        let text = cfg(3001).render().unwrap();
        assert!(text.contains("\"uri\"        \"http://127.0.0.1:3001/\""));
        assert!(text.contains("\"heartbeat\"  \"1.0\""));
        for b in M1_DATA_BLOCKS {
            assert!(text.contains(&format!("\"{b}\" \"1\"")));
        }
        assert!(!text.contains("abilities") && !text.contains("items"));
        assert!(!text.contains('\r'));
        assert_eq!(
            cfg(3001).file_name().unwrap(),
            "gamestate_integration_kohbor.cfg"
        );
    }

    #[test]
    fn rejects_out_of_range_port_bad_slug_and_bad_token() {
        assert_eq!(cfg(2999).render(), Err(ConfigError::PortOutOfRange));
        assert_eq!(cfg(3011).render(), Err(ConfigError::PortOutOfRange));
        let mut c = cfg(3000);
        c.slug = "Bad-Slug";
        assert_eq!(c.render(), Err(ConfigError::InvalidSlug));
        let mut c = cfg(3000);
        c.token = "short";
        assert_eq!(c.render(), Err(ConfigError::InvalidToken));
    }
}
