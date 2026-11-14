//! Read-only installed-game portraits. No extracted image is persisted.

use base64::Engine;
use flate2::{write::ZlibEncoder, Compression};
use kohbor_game_assets::{decode_vtex_c, hero_portrait_path, Image, Vpk};
use serde::Serialize;
use std::collections::{HashSet, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

const PORTRAIT_CACHE_CAPACITY: usize = 40;
const HERO_ID_PREFIX: &str = "npc_dota_hero_";
const MAX_PORTRAIT_WIDTH: u32 = 128;
const MAX_IMAGE_DIMENSION: u32 = 256;
const MAX_ENCODED_PNG_BYTES: usize = 256 * 1024;
const MAX_WARNED_HERO_IDS: usize = 256;

static PORTRAITS: OnceLock<Arc<PortraitService>> = OnceLock::new();

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct PortraitResponse {
    width: u32,
    height: u32,
    mime: &'static str,
    base64: String,
}

fn valid_hero_id(hero_id: &str) -> bool {
    let Some(suffix) = hero_id.strip_prefix(HERO_ID_PREFIX) else {
        return false;
    };
    (1..=40).contains(&suffix.len())
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

struct LruCache<T> {
    capacity: usize,
    entries: VecDeque<(String, T)>,
}

impl<T> LruCache<T> {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: VecDeque::with_capacity(capacity),
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }

    fn get(&mut self, key: &str) -> Option<&T> {
        let index = self.entries.iter().position(|(known, _)| known == key)?;
        let entry = self.entries.remove(index)?;
        self.entries.push_back(entry);
        self.entries.back().map(|(_, value)| value)
    }

    fn insert(&mut self, key: String, value: T) {
        if self.capacity == 0 {
            return;
        }
        if let Some(index) = self.entries.iter().position(|(known, _)| known == &key) {
            self.entries.remove(index);
        }
        while self.entries.len() >= self.capacity {
            self.entries.pop_front();
        }
        self.entries.push_back((key, value));
    }
}

struct PortraitState {
    vpk: Option<Vpk>,
    open_attempted: bool,
    open_failure: &'static str,
    cache: LruCache<PortraitResponse>,
    // Only validated, bounded-length IDs enter this bounded set; none are logged.
    warned: HashSet<String>,
}

impl PortraitState {
    fn warn_once(&mut self, hero_id: &str, reason: &'static str) {
        if valid_hero_id(hero_id)
            && self.warned.len() < MAX_WARNED_HERO_IDS
            && self.warned.insert(hero_id.to_owned())
        {
            crate::log_warn!(
                "portraits",
                "portrait_unavailable",
                "Local hero portrait unavailable",
                reason = reason
            );
        }
    }
}

struct PortraitService {
    discover: Box<dyn Fn() -> Option<PathBuf> + Send + Sync>,
    state: Mutex<PortraitState>,
}

impl PortraitService {
    fn new(discover: impl Fn() -> Option<PathBuf> + Send + Sync + 'static) -> Self {
        Self {
            discover: Box::new(discover),
            state: Mutex::new(PortraitState {
                vpk: None,
                open_attempted: false,
                open_failure: "VPK_UNAVAILABLE",
                cache: LruCache::new(PORTRAIT_CACHE_CAPACITY),
                warned: HashSet::new(),
            }),
        }
    }

    /// Called only by the blocking worker in production. The VPK handle and
    /// cache share one mutex so concurrent requests cannot open duplicate VPKs.
    fn load(&self, hero_id: &str) -> Option<PortraitResponse> {
        if !valid_hero_id(hero_id) {
            return None;
        }
        let mut state = crate::intake::lock(&self.state);
        if let Some(cached) = state.cache.get(hero_id) {
            return Some(cached.clone());
        }
        if !state.open_attempted {
            state.open_attempted = true;
            match (self.discover)() {
                Some(game) => {
                    state.vpk = Vpk::open(game.join("game/dota/pak01_dir.vpk")).ok();
                }
                None => state.open_failure = "INSTALL_UNAVAILABLE",
            }
        }
        let result = (|| {
            let vpk = state.vpk.as_ref().ok_or(state.open_failure)?;
            // The exact host validator runs before the crate builds any path.
            // This command intentionally uses only the upright selection image.
            let path = hero_portrait_path(hero_id)
                .into_iter()
                .find(|path| path.starts_with("panorama/images/heroes/selection/"))
                .ok_or("INVALID_HERO_ID")?;
            let entry = vpk.find(&path).ok_or("ENTRY_UNAVAILABLE")?;
            let bytes = vpk.read(&entry).map_err(|_| "READ_FAILED")?;
            let image = decode_vtex_c(&bytes).map_err(|_| "DECODE_FAILED")?;
            let image = resize_portrait(image).ok_or("RESIZE_FAILED")?;
            let png = encode_png(&image).ok_or("PNG_ENCODE_FAILED")?;
            Ok::<_, &'static str>(PortraitResponse {
                width: image.width,
                height: image.height,
                mime: "image/png",
                base64: base64::engine::general_purpose::STANDARD.encode(png),
            })
        })();
        match result {
            Ok(portrait) => {
                state.cache.insert(hero_id.to_owned(), portrait.clone());
                Some(portrait)
            }
            Err(reason) => {
                state.warn_once(hero_id, reason);
                None
            }
        }
    }
}

async fn load_on_worker(
    service: Arc<PortraitService>,
    hero_id: String,
) -> Option<PortraitResponse> {
    if !valid_hero_id(&hero_id) {
        return None;
    }
    let failed_hero_id = hero_id.clone();
    let failures = service.clone();
    match tauri::async_runtime::spawn_blocking(move || service.load(&hero_id)).await {
        Ok(result) => result,
        Err(_) => {
            crate::intake::lock(&failures.state).warn_once(&failed_hero_id, "WORKER_FAILED");
            None
        }
    }
}

#[tauri::command(rename_all = "snake_case")]
pub(crate) async fn hero_portrait(hero_id: String) -> Option<PortraitResponse> {
    let service = PORTRAITS
        .get_or_init(|| Arc::new(PortraitService::new(|| crate::intake::locate_game().ok())))
        .clone();
    load_on_worker(service, hero_id).await
}

fn resize_portrait(image: Image) -> Option<Image> {
    if !bounded_dimensions(image.width, image.height) {
        return None;
    }
    let expected = usize::try_from(image.width)
        .ok()?
        .checked_mul(usize::try_from(image.height).ok()?)?
        .checked_mul(4)?;
    if image.width == 0 || image.height == 0 || expected != image.rgba.len() {
        return None;
    }
    if image.width <= MAX_PORTRAIT_WIDTH {
        return Some(image);
    }
    let width = MAX_PORTRAIT_WIDTH;
    let height =
        (u64::from(image.height) * u64::from(width) / u64::from(image.width)).max(1) as u32;
    if !bounded_dimensions(width, height) {
        return None;
    }
    let length = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let mut rgba = Vec::new();
    rgba.try_reserve_exact(length).ok()?;
    for y in 0..height {
        let source_y = u64::from(y) * u64::from(image.height) / u64::from(height);
        for x in 0..width {
            let source_x = u64::from(x) * u64::from(image.width) / u64::from(width);
            let offset =
                usize::try_from((source_y * u64::from(image.width) + source_x) * 4).ok()?;
            rgba.extend_from_slice(image.rgba.get(offset..offset.checked_add(4)?)?);
        }
    }
    Some(Image {
        width,
        height,
        rgba,
    })
}

fn encode_png(image: &Image) -> Option<Vec<u8>> {
    if !bounded_dimensions(image.width, image.height) {
        return None;
    }
    let row_bytes = (image.width as usize).checked_mul(4)?;
    if row_bytes == 0
        || image.height == 0
        || row_bytes.checked_mul(image.height as usize)? != image.rgba.len()
    {
        return None;
    }
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
    for row in image.rgba.chunks_exact(row_bytes) {
        encoder.write_all(&[0]).ok()?; // PNG filter None, RGBA8.
        encoder.write_all(row).ok()?;
    }
    let compressed = encoder.finish().ok()?;
    let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = Vec::with_capacity(13);
    header.extend_from_slice(&image.width.to_be_bytes());
    header.extend_from_slice(&image.height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut png, b"IHDR", &header)?;
    png_chunk(&mut png, b"IDAT", &compressed)?;
    png_chunk(&mut png, b"IEND", &[])?;
    Some(png)
}

fn png_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Option<()> {
    if png.len().checked_add(data.len())?.checked_add(12)? > MAX_ENCODED_PNG_BYTES {
        return None;
    }
    png.extend_from_slice(&u32::try_from(data.len()).ok()?.to_be_bytes());
    let crc_start = png.len();
    png.extend_from_slice(kind);
    png.extend_from_slice(data);
    let mut crc = !0_u32;
    for byte in &png[crc_start..] {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    png.extend_from_slice(&(!crc).to_be_bytes());
    Some(())
}

fn bounded_dimensions(width: u32, height: u32) -> bool {
    (1..=MAX_IMAGE_DIMENSION).contains(&width) && (1..=MAX_IMAGE_DIMENSION).contains(&height)
}

#[cfg(test)]
mod tests {
    use super::{
        load_on_worker, valid_hero_id, LruCache, PortraitResponse, PortraitService,
        PORTRAIT_CACHE_CAPACITY,
    };
    use base64::Engine;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    static SERVICE_TEST_GATE: Mutex<()> = Mutex::new(());

    #[test]
    fn ten_thousand_invalid_ids_never_enter_warning_storage_or_discovery() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let service = PortraitService::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
            None
        });
        let logs = crate::logging::collect_test_logs(|| {
            for index in 0..10_000 {
                assert!(service.load(&format!("../invalid-{index}")).is_none());
            }
        });
        assert_eq!(crate::intake::lock(&service.state).warned.len(), 0);
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        assert!(logs.is_empty());
    }

    #[test]
    fn warnings_remember_at_most_256_valid_ids_and_silently_drop_further_ids() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        let service = PortraitService::new(|| None);
        let logs = crate::logging::collect_test_logs(|| {
            for index in 0..300 {
                assert!(service
                    .load(&format!("npc_dota_hero_warn_{index}"))
                    .is_none());
            }
            assert!(service.load("npc_dota_hero_warn_0").is_none());
            assert!(service.load("npc_dota_hero_warn_299").is_none());
        });
        assert_eq!(crate::intake::lock(&service.state).warned.len(), 256);
        assert_eq!(logs.len(), 256);
    }

    #[test]
    fn decoded_and_output_dimensions_are_bounded_and_keep_the_aspect_ratio() {
        for (width, height) in [(0, 1), (1, 0), (257, 1), (1, 257), (1, 100_000)] {
            let image = kohbor_game_assets::Image {
                width,
                height,
                rgba: vec![42; width as usize * height as usize * 4],
            };
            assert!(
                super::resize_portrait(image).is_none(),
                "accepted {width} x {height}"
            );
        }
        for (width, height, expected) in [
            (256, 128, (128, 64)),
            (256, 256, (128, 128)),
            (128, 256, (128, 256)),
            (64, 128, (64, 128)),
        ] {
            let resized = super::resize_portrait(kohbor_game_assets::Image {
                width,
                height,
                rgba: vec![42; width as usize * height as usize * 4],
            })
            .unwrap();
            assert_eq!((resized.width, resized.height), expected);
            assert!(resized.width <= 256 && resized.height <= 256);
            assert_eq!(
                u64::from(width) * u64::from(resized.height),
                u64::from(height) * u64::from(resized.width)
            );
        }
    }

    #[test]
    fn png_rejects_oversized_dimensions_and_more_than_256_kib() {
        let oversized = kohbor_game_assets::Image {
            width: 1,
            height: 257,
            rgba: vec![0; 257 * 4],
        };
        assert!(super::encode_png(&oversized).is_none());
        let mut random = 0x1234_5678_u32;
        let rgba: Vec<u8> = (0..256 * 256 * 4)
            .map(|_| {
                random ^= random << 13;
                random ^= random >> 17;
                random ^= random << 5;
                random as u8
            })
            .collect();
        let noisy = kohbor_game_assets::Image {
            width: 256,
            height: 256,
            rgba,
        };
        assert!(
            super::encode_png(&noisy).is_none(),
            "incompressible PNG exceeded the byte cap"
        );
        let small = kohbor_game_assets::Image {
            width: 128,
            height: 256,
            rgba: noisy.rgba[..128 * 256 * 4].to_vec(),
        };
        assert!(super::encode_png(&small).is_some_and(|png| png.len() <= 256 * 1024));
    }

    #[test]
    fn command_worker_opens_lazily_and_returns_the_resized_selection_png() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        let game = TempGame::new();
        let mut pixels = Vec::new();
        for y in 0..128_u16 {
            for x in 0..256_u16 {
                pixels.extend_from_slice(&[x as u8, y as u8, 9, 255]);
            }
        }
        game.write_vpk(vec![
            (selection_path("axe"), make_vtex(256, 128, 28, &pixels)),
            (
                "panorama/images/heroes/npc_dota_hero_axe_png.vtex_c".into(),
                make_vtex(1, 1, 28, &[99, 99, 99, 255]),
            ),
        ]);
        let original = std::fs::read(game.vpk_path()).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed_thread = Arc::new(Mutex::new(None));
        let root = game.root.clone();
        let count = calls.clone();
        let thread = observed_thread.clone();
        let service = Arc::new(PortraitService::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
            *crate::intake::lock(&thread) = Some(std::thread::current().id());
            Some(root.clone())
        }));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        let caller = std::thread::current().id();
        let result = tauri::async_runtime::block_on(load_on_worker(
            service.clone(),
            "npc_dota_hero_axe".into(),
        ))
        .unwrap();
        assert_eq!(
            (result.width, result.height, result.mime),
            (128, 64, "image/png")
        );
        assert_ne!(*crate::intake::lock(&observed_thread), Some(caller));
        let png = base64::engine::general_purpose::STANDARD
            .decode(&result.base64)
            .unwrap();
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        let decoded = kohbor_game_assets::decode_vtex_c(&make_vtex(128, 64, 16, &png)).unwrap();
        assert_eq!(&decoded.rgba[(128 + 1) * 4..(128 + 2) * 4], &[2, 2, 9, 255]);
        assert_eq!(service.load("npc_dota_hero_axe"), Some(result));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(std::fs::read(game.vpk_path()).unwrap(), original);
        assert_eq!(
            std::fs::read_dir(game.root.join("game/dota"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn invalid_and_unavailable_install_paths_return_none_with_one_warning_per_id() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        let calls = Arc::new(AtomicUsize::new(0));
        let count = calls.clone();
        let service = PortraitService::new(move || {
            count.fetch_add(1, Ordering::Relaxed);
            None
        });
        let logs = crate::logging::collect_test_logs(|| {
            for _ in 0..2 {
                assert!(service.load("../unchecked").is_none());
            }
            assert_eq!(calls.load(Ordering::Relaxed), 0);
            for _ in 0..2 {
                assert!(service.load("npc_dota_hero_axe").is_none());
            }
        });
        assert_eq!(logs.len(), 1);
        assert!(logs
            .iter()
            .all(|entry| entry.level == crate::logging::Level::Warn
                && entry.event == "portrait_unavailable"));
        assert!(!serde_json::to_string(&logs)
            .unwrap()
            .contains("../unchecked"));
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert_eq!(crate::intake::lock(&service.state).warned.len(), 1);

        let game = TempGame::new();
        let service = PortraitService::new(move || Some(game.root.clone()));
        assert!(service.load("npc_dota_hero_axe").is_none());
    }

    #[test]
    fn missing_entry_bad_vpk_and_bad_texture_are_none_without_panics() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        for entries in [vec![], vec![(selection_path("axe"), vec![1, 2, 3])]] {
            let game = TempGame::new();
            game.write_vpk(entries);
            let root = game.root.clone();
            let service = PortraitService::new(move || Some(root.clone()));
            for _ in 0..2 {
                assert!(service.load("npc_dota_hero_axe").is_none());
            }
            assert_eq!(crate::intake::lock(&service.state).warned.len(), 1);
        }
        let game = TempGame::new();
        std::fs::write(game.vpk_path(), b"invalid VPK").unwrap();
        let root = game.root.clone();
        let service = PortraitService::new(move || Some(root.clone()));
        assert!(service.load("npc_dota_hero_axe").is_none());
    }

    #[test]
    fn service_keeps_at_most_forty_portraits_and_serves_hits_without_disk_files() {
        let _gate = crate::intake::lock(&SERVICE_TEST_GATE);
        let game = TempGame::new();
        game.write_vpk(
            (0..41)
                .map(|n| {
                    (
                        selection_path(&format!("test_{n}")),
                        make_vtex(1, 1, 28, &[1, 2, 3, 255]),
                    )
                })
                .collect(),
        );
        let root = game.root.clone();
        let service = PortraitService::new(move || Some(root.clone()));
        for n in 0..40 {
            assert!(service.load(&format!("npc_dota_hero_test_{n}")).is_some());
        }
        assert!(service.load("npc_dota_hero_test_1").is_some());
        assert!(service.load("npc_dota_hero_test_40").is_some());
        assert_eq!(crate::intake::lock(&service.state).cache.len(), 40);
        std::fs::remove_file(game.vpk_path()).unwrap();
        assert!(service.load("npc_dota_hero_test_1").is_some());
        assert!(service.load("npc_dota_hero_test_0").is_none());
        assert_eq!(
            std::fs::read_dir(game.root.join("game/dota"))
                .unwrap()
                .count(),
            0
        );
    }

    struct TempGame {
        root: PathBuf,
    }
    impl TempGame {
        fn new() -> Self {
            static NEXT: AtomicUsize = AtomicUsize::new(0);
            let temp = std::env::temp_dir().canonicalize().unwrap();
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = temp.join(format!(
                "kohbor-host3-{}-{nonce}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::create_dir(&root).unwrap();
            std::fs::create_dir_all(root.join("game/dota")).unwrap();
            Self { root }
        }
        fn vpk_path(&self) -> PathBuf {
            self.root.join("game/dota/pak01_dir.vpk")
        }
        fn write_vpk(&self, entries: Vec<(String, Vec<u8>)>) {
            let mut groups: BTreeMap<String, Vec<(String, Vec<u8>)>> = BTreeMap::new();
            for (path, bytes) in entries {
                let (dir, file) = path.rsplit_once('/').unwrap();
                groups
                    .entry(dir.into())
                    .or_default()
                    .push((file.strip_suffix(".vtex_c").unwrap().into(), bytes));
            }
            let mut tree = Vec::new();
            let mut data = Vec::new();
            if !groups.is_empty() {
                cstr(&mut tree, "vtex_c");
                for (dir, entries) in groups {
                    cstr(&mut tree, &dir);
                    for (name, bytes) in entries {
                        cstr(&mut tree, &name);
                        tree.extend_from_slice(&test_crc32(&bytes).to_le_bytes());
                        tree.extend_from_slice(&0_u16.to_le_bytes());
                        tree.extend_from_slice(&0x7fff_u16.to_le_bytes());
                        tree.extend_from_slice(&(data.len() as u32).to_le_bytes());
                        tree.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
                        tree.extend_from_slice(&0xffff_u16.to_le_bytes());
                        data.extend_from_slice(&bytes);
                    }
                    tree.push(0);
                }
                tree.push(0);
            }
            tree.push(0);
            let mut vpk = vec![0_u8; 28];
            vpk[..4].copy_from_slice(&0x55aa1234_u32.to_le_bytes());
            vpk[4..8].copy_from_slice(&2_u32.to_le_bytes());
            vpk[8..12].copy_from_slice(&(tree.len() as u32).to_le_bytes());
            vpk[12..16].copy_from_slice(&(data.len() as u32).to_le_bytes());
            vpk.extend_from_slice(&tree);
            vpk.extend_from_slice(&data);
            std::fs::write(self.vpk_path(), vpk).unwrap();
        }
    }
    impl Drop for TempGame {
        fn drop(&mut self) {
            // Delete only this test's newly created child of the resolved temp root.
            if let (Ok(root), Ok(temp)) = (
                self.root.canonicalize(),
                std::env::temp_dir().canonicalize(),
            ) {
                if root.parent() == Some(temp.as_path())
                    && root
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("kohbor-host3-"))
                {
                    let _ = std::fs::remove_dir_all(root);
                }
            }
        }
    }
    fn cstr(target: &mut Vec<u8>, text: &str) {
        target.extend_from_slice(text.as_bytes());
        target.push(0);
    }
    fn selection_path(hero: &str) -> String {
        format!("panorama/images/heroes/selection/npc_dota_hero_{hero}_png.vtex_c")
    }
    fn test_crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0_u32;
        for byte in bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb88320 & 0_u32.wrapping_sub(crc & 1));
            }
        }
        !crc
    }
    fn make_vtex(width: u16, height: u16, format: u8, payload: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0_u8; 124];
        bytes[..4].copy_from_slice(&124_u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0001_000c_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&8_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&1_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(b"DATA");
        bytes[20..24].copy_from_slice(&76_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&28_u32.to_le_bytes());
        bytes[96..98].copy_from_slice(&1_u16.to_le_bytes());
        bytes[116..118].copy_from_slice(&width.to_le_bytes());
        bytes[118..120].copy_from_slice(&height.to_le_bytes());
        bytes[120..122].copy_from_slice(&1_u16.to_le_bytes());
        bytes[122] = format;
        bytes[123] = 1;
        bytes.extend_from_slice(payload);
        bytes
    }

    #[test]
    fn hero_identity_validation_accepts_only_the_bounded_exact_form() {
        assert!(valid_hero_id("npc_dota_hero_axe"));
        assert!(valid_hero_id(&format!("npc_dota_hero_{}", "a".repeat(40))));
        for invalid in [
            "axe",
            "npc_dota_hero_",
            "npc_dota_hero_Axe",
            "npc_dota_hero_foo-bar",
            "npc_dota_hero_../foo",
            "npc_dota_hero_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(
                !valid_hero_id(invalid),
                "unexpected valid hero id: {invalid}"
            );
        }
    }

    #[test]
    fn portrait_lru_is_bounded_and_promotes_cache_hits() {
        let mut cache = LruCache::new(PORTRAIT_CACHE_CAPACITY);
        for index in 0..=PORTRAIT_CACHE_CAPACITY {
            cache.insert(format!("hero-{index}"), index);
        }
        assert_eq!(cache.len(), PORTRAIT_CACHE_CAPACITY);
        assert!(cache.get("hero-0").is_none());
        assert_eq!(cache.get("hero-1"), Some(&1));
        cache.insert("hero-last".to_owned(), 99);
        assert_eq!(cache.len(), PORTRAIT_CACHE_CAPACITY);
        assert!(cache.get("hero-2").is_none());
        assert_eq!(cache.get("hero-1"), Some(&1));
    }

    #[test]
    fn portrait_command_response_has_the_declared_image_shape() {
        assert_eq!(
            serde_json::to_value(tauri::async_runtime::block_on(super::hero_portrait(
                "bad".into()
            )))
            .unwrap(),
            serde_json::Value::Null
        );
        let value = serde_json::to_value(PortraitResponse {
            width: 128,
            height: 64,
            mime: "image/png",
            base64: "AA==".to_owned(),
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "width": 128,
                "height": 64,
                "mime": "image/png",
                "base64": "AA=="
            })
        );
    }
}
