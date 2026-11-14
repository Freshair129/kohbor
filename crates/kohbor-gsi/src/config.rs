//! Owned game-integration configuration (GSI-INTAKE-SPEC §2).

/// First and last candidate loopback ports (inclusive).
pub const PORT_FIRST: u16 = 3000;
pub const PORT_LAST: u16 = 3010;
/// Maximum accepted request body in bytes (GSI-INTAKE-SPEC §3).
pub const BODY_LIMIT_BYTES: usize = 131_072;
/// Requests above this rate are dropped after authentication.
pub const MAX_REQUESTS_PER_SECOND: u32 = 50;

/// Data blocks requested for the local own-hero read model.
pub const OWN_DATA_BLOCKS: [&str; 6] = ["provider", "map", "player", "hero", "abilities", "items"];

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

/// Loopback ports claimed by other applications' GSI configuration files.
///
/// Capture 2026-11-11 found another application's file pointing at port 3000 while
/// nothing listened there, so "free right now" is not enough (GSI-INTAKE-SPEC §2.4).
pub fn claimed_ports<'a>(other_config_texts: impl IntoIterator<Item = &'a str>) -> Vec<u16> {
    let mut ports: Vec<u16> = other_config_texts
        .into_iter()
        .flat_map(str::lines)
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix("\"uri\"")?.trim().trim_matches('"');
            let host_port = rest
                .strip_prefix("http://")
                .or_else(|| rest.strip_prefix("https://"))?;
            let authority = host_port.split('/').next()?;
            let (host, port) = authority.rsplit_once(':')?;
            matches!(host, "127.0.0.1" | "localhost" | "[::1]")
                .then(|| port.parse::<u16>().ok())
                .flatten()
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// First candidate in 3000..=3010 that no other configuration claims and `is_free` accepts.
pub fn choose_port(claimed: &[u16], is_free: impl Fn(u16) -> bool) -> Option<u16> {
    (PORT_FIRST..=PORT_LAST).find(|p| !claimed.contains(p) && is_free(*p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claimed_ports_reads_other_configs_and_choose_port_skips_them() {
        let other = [
            "\"A\"\n{\n    \"uri\"           \"http://127.0.0.1:3000/gsi\"\n}\n",
            "\"B\"\n{\n  \"uri\" \"http://127.0.0.1:43721\"\n}\n",
            "\"C\"\n{\n    \"uri\" \"http://localhost:3011/gsi\"\n    \"uri\" \"http://10.0.0.5:3002/\"\n}\n",
        ];
        assert_eq!(claimed_ports(other), vec![3000, 3011, 43721]);
        assert_eq!(choose_port(&[3000, 3011], |_| true), Some(3001));
        assert_eq!(choose_port(&[3000], |p| p != 3001), Some(3002));
        assert_eq!(choose_port(&[], |_| false), None);
    }

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO-_";

    fn cfg(port: u16) -> GsiConfig<'static> {
        GsiConfig {
            display_name: "Kohbor",
            slug: "kohbor",
            port,
            token: TOKEN,
            data_blocks: &OWN_DATA_BLOCKS,
        }
    }

    #[test]
    fn renders_loopback_uri_heartbeat_and_all_own_data_blocks() {
        let text = cfg(3001).render().unwrap();
        assert!(text.contains("\"uri\"        \"http://127.0.0.1:3001/\""));
        assert!(text.contains("\"heartbeat\"  \"1.0\""));
        for b in OWN_DATA_BLOCKS {
            assert!(text.contains(&format!("\"{b}\" \"1\"")));
        }
        // DEC-OWN-SNAPSHOT-2-1 authorizes these own-hero blocks locally.
        assert!(text.contains("\"abilities\" \"1\""));
        assert!(text.contains("\"items\" \"1\""));
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
