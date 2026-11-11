//! M0 GSI capture spike (spike tool, not the product).
//!
//! Commands:
//!   locate                         read-only: Steam path, Dota folder, existing GSI files, game running
//!   install [--port N] [--out D]   write gamestate_integration_kohbor_spike.cfg (new file only)
//!   capture [--minutes M] [--out D] record redacted payloads + summary for A-1..A-9, E-1/E-2
//!   uninstall [--out D]            remove the spike config only if its hash is unchanged
//!
//! Redaction: auth token dropped; player.steamid/accountid/name replaced; map.matchid
//! replaced by a per-run salted hash prefix. Raw identifiers never reach disk.

use base64::Engine;
use kohbor_gsi::config::{
    choose_port, claimed_ports, GsiConfig, BODY_LIMIT_BYTES, M1_DATA_BLOCKS, PORT_FIRST, PORT_LAST,
};
use kohbor_gsi::strict_json;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const SPIKE_SLUG: &str = "kohbor_spike";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let opt = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let out = PathBuf::from(opt("--out").unwrap_or_else(|| "spike-out".into()));
    let result = match cmd {
        "locate" => locate().map(|_| ()),
        "install" => install(&out, opt("--port").and_then(|p| p.parse().ok())),
        "capture" => capture(
            &out,
            opt("--minutes").and_then(|m| m.parse().ok()).unwrap_or(10),
        ),
        "uninstall" => uninstall(&out),
        _ => {
            eprintln!("usage: kohbor-gsi-capture <locate|install [--port N]|capture [--minutes M]|uninstall> [--out DIR]");
            Ok(())
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

type Res<T> = Result<T, String>;

struct Located {
    steam: PathBuf,
    game: PathBuf,
    gsi_dir: PathBuf,
}

#[cfg(windows)]
fn steam_path() -> Option<PathBuf> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = hkcu.open_subkey(r"Software\Valve\Steam").ok()?;
    key.get_value::<String, _>("SteamPath")
        .ok()
        .map(PathBuf::from)
}
#[cfg(not(windows))]
fn steam_path() -> Option<PathBuf> {
    None
}

#[cfg(windows)]
fn active_account_id() -> Option<u32> {
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let key = hkcu
        .open_subkey(r"Software\Valve\Steam\ActiveProcess")
        .ok()?;
    key.get_value::<u32, _>("ActiveUser")
        .ok()
        .filter(|v| *v != 0)
}
#[cfg(not(windows))]
fn active_account_id() -> Option<u32> {
    None
}

fn library_paths(steam: &Path) -> Vec<PathBuf> {
    let mut libs = vec![steam.to_path_buf()];
    if let Ok(text) = fs::read_to_string(steam.join("steamapps/libraryfolders.vdf")) {
        for line in text.lines() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("\"path\"") {
                let v = rest.trim().trim_matches('"').replace("\\\\", "\\");
                if !v.is_empty() {
                    libs.push(PathBuf::from(v));
                }
            }
        }
    }
    libs.sort();
    libs.dedup();
    libs
}

fn install_dir(manifest: &Path) -> Option<String> {
    let text = fs::read_to_string(manifest).ok()?;
    text.lines().find_map(|l| {
        let t = l.trim();
        t.strip_prefix("\"installdir\"")
            .map(|r| r.trim().trim_matches('"').to_owned())
    })
}

fn find_game() -> Res<Located> {
    let steam = steam_path().ok_or("A-1: SteamPath registry value not found")?;
    let found: Vec<PathBuf> = library_paths(&steam)
        .into_iter()
        .filter_map(|lib| {
            let dir = install_dir(&lib.join("steamapps/appmanifest_570.acf"))?;
            let game = lib.join("steamapps/common").join(dir);
            game.join("game/dota/cfg").is_dir().then_some(game)
        })
        .collect();
    match found.as_slice() {
        [game] => Ok(Located {
            steam,
            gsi_dir: game.join("game/dota/cfg/gamestate_integration"),
            game: game.clone(),
        }),
        [] => Err(
            "GSI_GAME_NOT_FOUND: no library contains appmanifest_570.acf with game/dota/cfg".into(),
        ),
        _ => {
            Err("more than one Dota installation found; the product asks the user to choose".into())
        }
    }
}

fn game_running() -> bool {
    std::process::Command::new("tasklist")
        .args(["/FI", "IMAGENAME eq dota2.exe", "/NH"])
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .to_lowercase()
                .contains("dota2.exe")
        })
        .unwrap_or(false)
}

fn locate() -> Res<Located> {
    let loc = find_game()?;
    println!("steam: {}", loc.steam.display());
    println!("game: {}", loc.game.display());
    println!("gsi dir: {}", loc.gsi_dir.display());
    if let Ok(rd) = fs::read_dir(&loc.gsi_dir) {
        for e in rd.flatten() {
            println!("  existing: {}", e.file_name().to_string_lossy());
        }
    }
    println!(
        "platform identity available (A-7): {}",
        active_account_id().is_some()
    );
    println!("dota2.exe running: {}", game_running());
    Ok(loc)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn random_bytes<const N: usize>() -> Res<[u8; N]> {
    let mut b = [0u8; N];
    getrandom::getrandom(&mut b).map_err(|e| format!("rng: {e}"))?;
    Ok(b)
}

fn port_free(port: u16) -> bool {
    std::net::TcpListener::bind(("127.0.0.1", port)).is_ok()
}

fn install(out: &Path, port: Option<u16>) -> Res<()> {
    let loc = find_game()?;
    let port = match port {
        Some(p) if (PORT_FIRST..=PORT_LAST).contains(&p) && port_free(p) => p,
        Some(p) => {
            return Err(format!(
                "GSI_PORT_UNAVAILABLE: port {p} is outside 3000-3010 or in use"
            ))
        }
        None => {
            let others: Vec<String> = fs::read_dir(&loc.gsi_dir)
                .map(|rd| {
                    rd.flatten()
                        .filter(|e| !e.file_name().to_string_lossy().contains(SPIKE_SLUG))
                        .filter_map(|e| fs::read_to_string(e.path()).ok())
                        .collect()
                })
                .unwrap_or_default();
            let claimed = claimed_ports(others.iter().map(String::as_str));
            choose_port(&claimed, port_free).ok_or("GSI_PORT_EXHAUSTED")?
        }
    };
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>()?);
    let cfg = GsiConfig {
        display_name: "Kohbor spike",
        slug: SPIKE_SLUG,
        port,
        token: &token,
        data_blocks: &M1_DATA_BLOCKS,
    };
    let text = cfg.render().map_err(|e| format!("{e:?}"))?;
    let name = cfg.file_name().map_err(|e| format!("{e:?}"))?;
    fs::create_dir_all(&loc.gsi_dir).map_err(|e| format!("GSI_CONFIG_WRITE_FAILED: {e}"))?;
    let target = loc.gsi_dir.join(&name);
    if target.exists() {
        return Err(format!(
            "GSI_CONFIG_CONFLICT: {} already exists; run uninstall first",
            target.display()
        ));
    }
    let tmp = loc.gsi_dir.join(format!("{name}.tmp"));
    {
        let mut f = fs::File::create(&tmp).map_err(|e| format!("GSI_CONFIG_WRITE_FAILED: {e}"))?;
        f.write_all(text.as_bytes())
            .and_then(|_| f.sync_all())
            .map_err(|e| format!("GSI_CONFIG_WRITE_FAILED: {e}"))?;
    }
    fs::rename(&tmp, &target).map_err(|e| format!("GSI_CONFIG_WRITE_FAILED: {e}"))?;
    fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let state = json!({"port": port, "token": token, "config_path": target, "config_sha256": sha256_hex(text.as_bytes())});
    fs::write(
        out.join("spike-state.json"),
        serde_json::to_vec_pretty(&state).unwrap(),
    )
    .map_err(|e| e.to_string())?;
    println!("installed {} on port {port}", target.display());
    if game_running() {
        println!("Dota 2 is running: restart it so it reads the new configuration (A-4).");
    }
    Ok(())
}

fn read_state(out: &Path) -> Res<Value> {
    let bytes = fs::read(out.join("spike-state.json"))
        .map_err(|_| "no spike-state.json; run install first")?;
    serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn uninstall(out: &Path) -> Res<()> {
    let state = read_state(out)?;
    let path = PathBuf::from(state["config_path"].as_str().ok_or("bad state")?);
    let bytes = fs::read(&path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if sha256_hex(&bytes) != state["config_sha256"].as_str().unwrap_or("") {
        return Err("GSI_CONFIG_CONFLICT: file changed since install; not removing".into());
    }
    fs::remove_file(&path).map_err(|e| e.to_string())?;
    println!("removed {}", path.display());
    Ok(())
}

fn key_paths(v: &Value, prefix: &str, out: &mut BTreeSet<String>) {
    if let Value::Object(m) = v {
        for (k, child) in m {
            let p = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}.{k}")
            };
            out.insert(p.clone());
            key_paths(child, &p, out);
        }
    }
}

fn redact(mut body: Value, salt: &[u8]) -> Value {
    if let Value::Object(m) = &mut body {
        m.remove("auth");
        if let Some(Value::Object(p)) = m.get_mut("player") {
            for k in ["steamid", "accountid", "name"] {
                if p.contains_key(k) {
                    p.insert(k.into(), Value::String("<redacted>".into()));
                }
            }
        }
        if let Some(Value::Object(map)) = m.get_mut("map") {
            if let Some(Value::String(id)) = map.get("matchid").cloned() {
                let mut h = Sha256::new();
                h.update(salt);
                h.update(id.as_bytes());
                let short: String = h
                    .finalize()
                    .iter()
                    .take(4)
                    .map(|b| format!("{b:02x}"))
                    .collect();
                map.insert("matchid".into(), Value::String(format!("<match:{short}>")));
            }
        }
    }
    body
}

const SUMMARY_SCOPE: &str =
    "M0 capture spike evidence for DEC-GSI-INTAKE / DEC-EVENT-AUDIO; not application acceptance";

fn write_summary(out: &Path, summary: &Value) -> Res<()> {
    let tmp = out.join("summary.json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(summary).unwrap()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, out.join("summary.json")).map_err(|e| e.to_string())
}

fn capture(out: &Path, minutes: u64) -> Res<()> {
    let state = read_state(out)?;
    let port = state["port"].as_u64().ok_or("bad state")? as u16;
    let token = state["token"].as_str().ok_or("bad state")?.to_owned();
    let platform = active_account_id().map(kohbor_gsi::steamid64_from_account_id);
    let server = tiny_http::Server::http(("127.0.0.1", port))
        .map_err(|e| format!("GSI_PORT_UNAVAILABLE: {e}"))?;
    let salt = random_bytes::<16>()?;
    let mut log = fs::File::create(out.join("corpus.jsonl")).map_err(|e| e.to_string())?;
    let start = Instant::now();
    let deadline = start + Duration::from_secs(minutes * 60);
    let mut last_arrival: Option<Instant> = None;
    let mut last_body_hash: Option<String> = None;
    let (mut admitted, mut rejected, mut max_gap, mut max_gap_unchanged) =
        (0u64, 0u64, 0u128, 0u128);
    let mut fields = BTreeSet::new();
    let mut game_states = BTreeSet::new();
    let mut ownership_matches = BTreeSet::new();
    let mut hero_names_per_match: Map<String, Value> = Map::new();
    println!(
        "listening on 127.0.0.1:{port} for {minutes} min; summary.json is refreshed every 15 s"
    );
    let mut last_summary = Instant::now();
    while Instant::now() < deadline {
        if last_summary.elapsed() >= Duration::from_secs(15) {
            last_summary = Instant::now();
            write_summary(
                out,
                &json!({
                    "scope": SUMMARY_SCOPE,
                    "duration_s": start.elapsed().as_secs(),
                    "admitted_payloads": admitted,
                    "rejected_requests": rejected,
                    "A3_max_gap_ms": max_gap as u64,
                    "A3_max_gap_between_identical_payloads_ms": max_gap_unchanged as u64,
                    "A5_field_paths": fields,
                    "E2_game_states_seen": game_states,
                    "A7_platform_identity_available": platform.is_some(),
                    "A7_payload_matches_platform_identity": ownership_matches,
                    "A8_hero_names_per_match": hero_names_per_match,
                }),
            )?;
        }
        let Ok(Some(mut req)) = server.recv_timeout(Duration::from_millis(500)) else {
            continue;
        };
        let arrival = Instant::now();
        let mut body = Vec::new();
        let ok_method = *req.method() == tiny_http::Method::Post && req.url() == "/";
        let too_big = req.body_length().is_some_and(|n| n > BODY_LIMIT_BYTES);
        if ok_method && !too_big {
            let _ = req
                .as_reader()
                .take(BODY_LIMIT_BYTES as u64 + 1)
                .read_to_end(&mut body);
        }
        let parsed = (ok_method && !too_big && body.len() <= BODY_LIMIT_BYTES)
            .then(|| strict_json::parse(&body).ok())
            .flatten();
        let authed = parsed
            .as_ref()
            .and_then(|v| v.pointer("/auth/token").and_then(Value::as_str))
            .is_some_and(|t| kohbor_gsi::admission::token_matches(&token, t));
        let _ = req.respond(tiny_http::Response::empty(if authed { 200 } else { 401 }));
        let Some(v) = parsed.filter(|_| authed) else {
            rejected += 1;
            continue;
        };
        admitted += 1;
        let gap = last_arrival
            .map(|l| arrival.duration_since(l).as_millis())
            .unwrap_or(0);
        let mut no_token = v.clone();
        if let Value::Object(m) = &mut no_token {
            m.remove("auth");
            if let Some(Value::Object(p)) = m.get_mut("provider") {
                p.remove("timestamp");
            }
        }
        let body_hash = sha256_hex(no_token.to_string().as_bytes());
        let unchanged = last_body_hash.as_deref() == Some(body_hash.as_str());
        max_gap = max_gap.max(gap);
        if unchanged {
            max_gap_unchanged = max_gap_unchanged.max(gap);
        }
        last_arrival = Some(arrival);
        last_body_hash = Some(body_hash);
        key_paths(&v, "", &mut fields);
        if let Some(s) = v.pointer("/map/game_state").and_then(Value::as_str) {
            game_states.insert(s.to_owned());
        }
        let payload_id = v
            .pointer("/player/steamid")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<u64>().ok());
        if let (Some(p), Some(id)) = (platform, payload_id) {
            ownership_matches.insert(p == id);
        }
        let red = redact(v, &salt);
        if let (Some(m), Some(h)) = (
            red.pointer("/map/matchid").and_then(Value::as_str),
            red.pointer("/hero/name").and_then(Value::as_str),
        ) {
            let entry = hero_names_per_match
                .entry(m.to_owned())
                .or_insert_with(|| json!([]));
            if let Value::Array(a) = entry {
                if !a.iter().any(|x| x == h) {
                    a.push(Value::String(h.to_owned()));
                }
            }
        }
        let line = json!({"t_ms": arrival.duration_since(start).as_millis() as u64, "gap_ms": gap as u64, "unchanged": unchanged, "bytes": body.len(), "body": red});
        writeln!(log, "{line}").map_err(|e| e.to_string())?;
    }
    let summary = json!({
        "scope": SUMMARY_SCOPE,
        "duration_s": start.elapsed().as_secs(),
        "admitted_payloads": admitted,
        "rejected_requests": rejected,
        "A3_max_gap_ms": max_gap as u64,
        "A3_max_gap_between_identical_payloads_ms": max_gap_unchanged as u64,
        "A5_field_paths": fields,
        "E2_game_states_seen": game_states,
        "A7_platform_identity_available": platform.is_some(),
        "A7_payload_matches_platform_identity": ownership_matches,
        "A8_hero_names_per_match": hero_names_per_match,
    });
    write_summary(out, &summary)?;
    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    Ok(())
}
