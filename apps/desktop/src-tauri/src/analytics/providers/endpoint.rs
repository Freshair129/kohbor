#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Endpoint {
    pub(crate) secure: bool,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) base_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RequestEndpoint {
    pub(crate) secure: bool,
    pub(crate) host: String,
    pub(crate) port: u16,
    pub(crate) path: String,
}

impl Endpoint {
    pub(crate) fn parse(value: &str) -> Result<Self, &'static str> {
        if value.is_empty()
            || value.len() > 2048
            || value
                .bytes()
                .any(|byte| byte.is_ascii_control() || byte.is_ascii_whitespace())
            || value.contains('\\')
            || value.contains('?')
            || value.contains('#')
            || value.contains('@')
        {
            return Err("endpoint_invalid");
        }
        let (scheme, remainder) = value.split_once("://").ok_or("endpoint_invalid")?;
        let secure = if scheme.eq_ignore_ascii_case("https") {
            true
        } else if scheme.eq_ignore_ascii_case("http") {
            false
        } else {
            return Err("endpoint_invalid");
        };
        let authority_end = remainder.find('/').unwrap_or(remainder.len());
        let authority = &remainder[..authority_end];
        let path = &remainder[authority_end..];
        if authority.is_empty() || !valid_base_path(path) {
            return Err("endpoint_invalid");
        }
        let (host, port) = parse_authority(authority, secure)?;
        let host = normalize_host(&host)?;
        let is_allowed_loopback = host == "127.0.0.1" || host == "::1" || host == "localhost";
        if !secure && !is_allowed_loopback {
            return Err("endpoint_invalid");
        }
        let host = if !secure && host == "localhost" {
            "127.0.0.1".to_owned()
        } else {
            host
        };
        Ok(Self {
            secure,
            host,
            port,
            base_path: path.trim_end_matches('/').to_owned(),
        })
    }

    pub(crate) fn is_loopback(&self) -> bool {
        matches!(self.host.as_str(), "127.0.0.1" | "::1" | "localhost")
    }

    pub(crate) fn origin(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let default_port = if self.secure { 443 } else { 80 };
        if self.port == default_port {
            format!("{}://{}", if self.secure { "https" } else { "http" }, host)
        } else {
            format!(
                "{}://{}:{}",
                if self.secure { "https" } else { "http" },
                host,
                self.port
            )
        }
    }

    pub(crate) fn path_with(&self, suffix: &str) -> String {
        if self.base_path.is_empty() {
            suffix.to_owned()
        } else {
            format!("{}{}", self.base_path, suffix)
        }
    }

    pub(crate) fn request_endpoint(&self, suffix: &str) -> RequestEndpoint {
        RequestEndpoint {
            secure: self.secure,
            host: self.host.clone(),
            port: self.port,
            path: self.path_with(suffix),
        }
    }
}

fn parse_authority(authority: &str, secure: bool) -> Result<(String, u16), &'static str> {
    let (host, port) = if let Some(bracketed) = authority.strip_prefix('[') {
        let close = bracketed.find(']').ok_or("endpoint_invalid")?;
        let host = &bracketed[..close];
        let address = host
            .parse::<std::net::Ipv6Addr>()
            .map_err(|_| "endpoint_invalid")?;
        let rest = &bracketed[close + 1..];
        let port = if rest.is_empty() {
            None
        } else {
            Some(
                rest.strip_prefix(':')
                    .ok_or("endpoint_invalid")?
                    .parse::<u16>()
                    .map_err(|_| "endpoint_invalid")?,
            )
        };
        (address.to_string(), port)
    } else {
        if authority.matches(':').count() > 1 {
            return Err("endpoint_invalid");
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, raw_port)) => (
                host,
                Some(raw_port.parse::<u16>().map_err(|_| "endpoint_invalid")?),
            ),
            None => (authority, None),
        };
        (host.to_owned(), port)
    };
    let port = port.unwrap_or(if secure { 443 } else { 80 });
    if port == 0 {
        return Err("endpoint_invalid");
    }
    Ok((host, port))
}

fn normalize_host(host: &str) -> Result<String, &'static str> {
    if host.is_empty() || host.ends_with('.') || host.contains('%') || host.contains(':') {
        if host.parse::<std::net::Ipv6Addr>().is_ok() {
            return Ok(host
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| "endpoint_invalid")?
                .to_string());
        }
        return Err("endpoint_invalid");
    }
    let labels = host
        .split('.')
        .map(punycode_label)
        .collect::<Result<Vec<_>, _>>()?;
    let normalized = labels.join(".");
    if normalized.len() > 253 {
        return Err("endpoint_invalid");
    }
    Ok(normalized)
}

fn punycode_label(label: &str) -> Result<String, &'static str> {
    if label.is_empty() {
        return Err("endpoint_invalid");
    }
    let label = label.to_lowercase();
    let points = label.chars().map(u32::from).collect::<Vec<_>>();
    if points.iter().all(|point| *point < 0x80) {
        let ascii = label;
        if !valid_ascii_label(&ascii) {
            return Err("endpoint_invalid");
        }
        return Ok(ascii);
    }
    if points
        .iter()
        .any(|point| *point >= 0x80 && !char::from_u32(*point).is_some_and(char::is_alphanumeric))
    {
        return Err("endpoint_invalid");
    }
    let mut output = String::from("xn--");
    let mut basic = 0_usize;
    for point in &points {
        if *point < 0x80 {
            let character = char::from_u32(*point).ok_or("endpoint_invalid")?;
            if !character.is_ascii_alphanumeric() && character != '-' {
                return Err("endpoint_invalid");
            }
            output.push(character);
            basic += 1;
        }
    }
    let mut handled = basic;
    if basic > 0 {
        output.push('-');
    }
    let mut n = 128_u64;
    let mut delta = 0_u64;
    let mut bias = 72_u64;
    while handled < points.len() {
        let next = points
            .iter()
            .copied()
            .map(u64::from)
            .filter(|point| *point >= n)
            .min()
            .ok_or("endpoint_invalid")?;
        delta = delta
            .checked_add(
                next.checked_sub(n)
                    .ok_or("endpoint_invalid")?
                    .checked_mul((handled + 1) as u64)
                    .ok_or("endpoint_invalid")?,
            )
            .ok_or("endpoint_invalid")?;
        n = next;
        for point in &points {
            let point = u64::from(*point);
            if point < n {
                delta = delta.checked_add(1).ok_or("endpoint_invalid")?;
            }
            if point == n {
                let mut q = delta;
                let mut k = 36_u64;
                loop {
                    let t = if k <= bias {
                        1
                    } else if k >= bias + 26 {
                        26
                    } else {
                        k - bias
                    };
                    if q < t {
                        break;
                    }
                    let digit = t + ((q - t) % (36 - t));
                    output.push(punycode_digit(digit)?);
                    q = (q - t) / (36 - t);
                    k = k.checked_add(36).ok_or("endpoint_invalid")?;
                }
                output.push(punycode_digit(q)?);
                bias = adapt_bias(delta, (handled + 1) as u64, handled == basic)?;
                delta = 0;
                handled += 1;
            }
        }
        delta = delta.checked_add(1).ok_or("endpoint_invalid")?;
        n = n.checked_add(1).ok_or("endpoint_invalid")?;
    }
    if output.len() > 63 || output.starts_with("xn---") {
        return Err("endpoint_invalid");
    }
    Ok(output)
}

fn valid_ascii_label(label: &str) -> bool {
    label.len() <= 63
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn punycode_digit(value: u64) -> Result<char, &'static str> {
    match value {
        0..=25 => char::from_u32(u32::from(b'a') + value as u32).ok_or("endpoint_invalid"),
        26..=35 => char::from_u32(u32::from(b'0') + (value - 26) as u32).ok_or("endpoint_invalid"),
        _ => Err("endpoint_invalid"),
    }
}

fn adapt_bias(mut delta: u64, points: u64, first: bool) -> Result<u64, &'static str> {
    delta = if first { delta / 700 } else { delta / 2 };
    delta = delta
        .checked_add(delta / points)
        .ok_or("endpoint_invalid")?;
    let mut k = 0_u64;
    while delta > 455 {
        delta /= 35;
        k = k.checked_add(36).ok_or("endpoint_invalid")?;
    }
    Ok(k + 36 * delta / (delta + 38))
}

fn valid_base_path(path: &str) -> bool {
    if path.is_empty() {
        return true;
    }
    if !path.starts_with('/') || path.starts_with("//") {
        return false;
    }
    let normalized = path.trim_end_matches('/');
    if normalized.is_empty() {
        return true;
    }
    if normalized
        .trim_start_matches('/')
        .split('/')
        .any(|part| part == "." || part == ".." || part.is_empty())
    {
        return false;
    }
    path.bytes().all(|byte| {
        byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~')
    })
}

#[cfg(test)]
mod tests {
    use super::Endpoint;

    #[test]
    fn https_host_is_normalized_and_keeps_explicit_port_and_base_path() {
        let endpoint = Endpoint::parse("HTTPS://BÜCHER.example:8443/v1").unwrap();
        assert_eq!(
            endpoint,
            Endpoint {
                secure: true,
                host: "xn--bcher-kva.example".into(),
                port: 8443,
                base_path: "/v1".into(),
            }
        );
    }

    #[test]
    fn http_is_limited_to_literal_loopback_hosts() {
        for input in [
            "http://127.0.0.1:11434",
            "http://[::1]:4000/v1",
            "http://localhost:8080",
        ] {
            assert!(Endpoint::parse(input).unwrap().is_loopback(), "{input}");
        }
        for input in [
            "http://192.168.1.5",
            "http://localhost.evil.test",
            "http://127.0.0.2",
            "http://example.test",
        ] {
            assert_eq!(
                Endpoint::parse(input).unwrap_err(),
                "endpoint_invalid",
                "{input}"
            );
        }
    }

    #[test]
    fn rejects_credentials_query_fragment_bad_ports_and_trailing_dot_hosts() {
        for input in [
            "https://user:pass@example.test/v1",
            "https://example.test/v1?x=1",
            "https://example.test/v1#frag",
            "https://example.test:0/v1",
            "https://example.test:65536/v1",
            "https://example.test./v1",
            "https://example.test\\v1",
        ] {
            assert!(Endpoint::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn joins_only_the_fixed_suffix_and_normalizes_localhost_transport_host() {
        let endpoint = Endpoint::parse("http://localhost:11434/v1").unwrap();
        assert_eq!(endpoint.host, "127.0.0.1");
        assert_eq!(
            endpoint.path_with("/chat/completions"),
            "/v1/chat/completions"
        );
        assert_eq!(
            Endpoint::parse("https://api.example.test")
                .unwrap()
                .path_with("/models"),
            "/models"
        );
    }
}
