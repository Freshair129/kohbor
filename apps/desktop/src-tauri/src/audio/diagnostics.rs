use crate::logging::Fields;
use serde_json::{json, Map, Value};

const STREAM_SAMPLE_RATE_HZ: u64 = 48_000;
const STREAM_CHANNELS: u64 = 1;
const STREAM_BITS_PER_SAMPLE: u64 = 16;

#[cfg(windows)]
pub(crate) fn native_playback_snapshot_json(playback_id: u64) -> Option<String> {
    kohbor_audio_native::WasapiSink::playback_snapshot_json(playback_id)
}

#[cfg(not(windows))]
pub(crate) fn native_playback_snapshot_json(_playback_id: u64) -> Option<String> {
    None
}

#[cfg(windows)]
pub(crate) fn render_thread_priority() -> Option<i32> {
    kohbor_audio_native::WasapiSink::render_thread_priority()
}

#[cfg(not(windows))]
pub(crate) fn render_thread_priority() -> Option<i32> {
    None
}

fn parse_snapshot(snapshot: Option<&str>) -> Option<Value> {
    serde_json::from_str(snapshot?).ok()
}

fn number_field(value: Option<&Value>, name: &str) -> Value {
    value
        .and_then(Value::as_object)
        .and_then(|object| object.get(name))
        .cloned()
        .unwrap_or(Value::Null)
}

fn number_ms(value: Option<u64>) -> Option<f64> {
    value.map(|nanoseconds| nanoseconds as f64 / 1_000_000.0)
}

fn signed_number_ms(value: Option<i64>) -> Option<f64> {
    value.map(|nanoseconds| nanoseconds as f64 / 1_000_000.0)
}

pub(crate) fn duration_ms(frames: Option<u64>) -> Option<f64> {
    frames.map(|frames| frames as f64 * 1_000.0 / STREAM_SAMPLE_RATE_HZ as f64)
}

pub(crate) fn clip_finished_fields(
    event_id: &str,
    reason: &str,
    expected_frames: Option<u64>,
    native_snapshot: Option<&str>,
    render_thread_priority: Option<i32>,
) -> Fields {
    let snapshot = parse_snapshot(native_snapshot);
    let snapshot_pending = snapshot
        .as_ref()
        .and_then(|value| value.get("snapshot_pending"))
        .and_then(Value::as_bool);
    let rendered_known = snapshot_pending != Some(true)
        && snapshot
            .as_ref()
            .and_then(|value| value.get("rendered_duration_known"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    let frames_written = rendered_known
        .then(|| {
            number_field(snapshot.as_ref(), "frames_written")
                .as_u64()
                .or_else(|| number_field(snapshot.as_ref(), "frames_rendered").as_u64())
        })
        .flatten();
    let mut fields = Map::new();
    fields.insert("event_id".to_owned(), Value::String(event_id.to_owned()));
    fields.insert("reason".to_owned(), Value::String(reason.to_owned()));
    fields.insert(
        "pcm_frames".to_owned(),
        number_field(snapshot.as_ref(), "pcm_frames"),
    );
    fields.insert(
        "cursor_frames".to_owned(),
        number_field(snapshot.as_ref(), "cursor_frames"),
    );
    fields.insert("frames_written".to_owned(), json!(frames_written));
    fields.insert(
        "write_error_count".to_owned(),
        if snapshot_pending == Some(true) {
            Value::Null
        } else {
            number_field(snapshot.as_ref(), "write_error_count")
        },
    );
    fields.insert(
        "event_timeout_count".to_owned(),
        number_field(snapshot.as_ref(), "event_timeout_count"),
    );
    fields.insert(
        "snapshot_pending".to_owned(),
        snapshot_pending.map_or(Value::Null, Value::Bool),
    );
    fields.insert(
        "clip_start_latency_ms".to_owned(),
        json!(number_ms(
            number_field(snapshot.as_ref(), "clip_start_latency_ns").as_u64()
        )),
    );
    fields.insert(
        "duration_expected_ms".to_owned(),
        json!(duration_ms(expected_frames)),
    );
    fields.insert(
        "duration_rendered_ms".to_owned(),
        json!(duration_ms(frames_written)),
    );
    fields.insert(
        "frames_available_total".to_owned(),
        number_field(snapshot.as_ref(), "frames_available_total"),
    );
    fields.insert(
        "frames_requested_total".to_owned(),
        number_field(snapshot.as_ref(), "frames_requested_total"),
    );
    fields.insert(
        "late_fills".to_owned(),
        number_field(snapshot.as_ref(), "late_fills"),
    );
    fields.insert(
        "max_lateness_ms".to_owned(),
        json!(signed_number_ms(
            number_field(snapshot.as_ref(), "max_lateness_ns").as_i64()
        )),
    );
    fields.insert(
        "min_padding_frames".to_owned(),
        number_field(snapshot.as_ref(), "min_padding_frames"),
    );
    fields.insert(
        "render_thread_priority".to_owned(),
        json!(render_thread_priority),
    );
    fields
}

pub(crate) fn device_open_fields(native_fields: Option<&str>) -> Fields {
    let native = parse_snapshot(native_fields);
    let mut fields = Map::new();
    fields.insert(
        "stream_sample_rate_hz".to_owned(),
        json!(STREAM_SAMPLE_RATE_HZ),
    );
    fields.insert("stream_channels".to_owned(), json!(STREAM_CHANNELS));
    fields.insert(
        "stream_bits_per_sample".to_owned(),
        json!(STREAM_BITS_PER_SAMPLE),
    );
    fields.insert("autoconvert".to_owned(), json!(true));
    for key in [
        "device_mix_sample_rate_hz",
        "device_mix_channels",
        "device_mix_bits_per_sample",
        "default_period_ms",
        "min_period_ms",
        "buffer_frames",
        "buffer_ms",
        "buffer_requested_ms",
        "buffer_fallback",
        "mmcss_task_index",
        "mmcss_ok",
        "mmcss_error_code",
        "priority_fallback_error_code",
        "render_thread_priority",
    ] {
        fields.insert(key.to_owned(), number_field(native.as_ref(), key));
    }
    fields
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn measured_snapshot(frames_rendered: u64) -> String {
        json!({
            "pcm_frames": frames_rendered,
            "cursor_frames": frames_rendered,
            "clip_start_latency_ns": 12_000_000_u64,
            "frames_rendered": frames_rendered,
            "frames_written": frames_rendered,
            "write_error_count": 0_u64,
            "event_timeout_count": 1_u64,
            "snapshot_pending": false,
            "rendered_duration_known": true,
            "frames_available_total": 48_000_u64,
            "frames_requested_total": 96_000_u64,
            "late_fills": 2_u64,
            "max_lateness_ns": 20_000_000_u64,
            "min_padding_frames": 3_u64,
        })
        .to_string()
    }

    #[test]
    fn full_clip_expected_and_rendered_durations_match() {
        let fields = clip_finished_fields(
            "hero_kill",
            "finished",
            Some(48_000),
            Some(&measured_snapshot(48_000)),
            Some(0),
        );

        assert_eq!(fields["duration_expected_ms"], json!(1_000.0));
        assert_eq!(fields["duration_rendered_ms"], json!(1_000.0));
    }

    #[test]
    fn interrupted_duration_reports_only_rendered_frames() {
        let fields = clip_finished_fields(
            "hero_kill",
            "interrupted",
            Some(48_000),
            Some(&measured_snapshot(12_000)),
            Some(0),
        );

        assert_eq!(fields["duration_expected_ms"], json!(1_000.0));
        assert_eq!(fields["duration_rendered_ms"], json!(250.0));
    }

    #[test]
    fn clip_finished_contains_all_diagnostic_fields_when_measured() {
        let fields = clip_finished_fields(
            "hero_kill",
            "finished",
            Some(48_000),
            Some(&measured_snapshot(48_000)),
            Some(0),
        );

        for key in [
            "event_id",
            "reason",
            "pcm_frames",
            "cursor_frames",
            "frames_written",
            "write_error_count",
            "event_timeout_count",
            "snapshot_pending",
            "clip_start_latency_ms",
            "duration_expected_ms",
            "duration_rendered_ms",
            "frames_available_total",
            "frames_requested_total",
            "late_fills",
            "max_lateness_ms",
            "min_padding_frames",
            "render_thread_priority",
        ] {
            assert!(fields.contains_key(key), "missing {key}");
        }
        assert_eq!(fields["clip_start_latency_ms"], json!(12.0));
        assert_eq!(fields["frames_available_total"], json!(48_000));
        assert_eq!(fields["frames_requested_total"], json!(96_000));
        assert_eq!(fields["pcm_frames"], json!(48_000));
        assert_eq!(fields["cursor_frames"], json!(48_000));
        assert_eq!(fields["frames_written"], json!(48_000));
        assert_eq!(fields["write_error_count"], json!(0));
        assert_eq!(fields["event_timeout_count"], json!(1));
        assert_eq!(fields["snapshot_pending"], json!(false));
        assert_eq!(fields["late_fills"], json!(2));
        assert_eq!(fields["max_lateness_ms"], json!(20.0));
        assert_eq!(fields["min_padding_frames"], json!(3));
        assert_eq!(fields["render_thread_priority"], json!(0));
    }

    #[test]
    fn unmeasured_clip_fields_are_null_and_expected_duration_remains_known() {
        let fields = clip_finished_fields("hero_kill", "interrupted", Some(48_000), None, None);

        for key in [
            "clip_start_latency_ms",
            "pcm_frames",
            "cursor_frames",
            "frames_written",
            "write_error_count",
            "event_timeout_count",
            "snapshot_pending",
            "duration_rendered_ms",
            "frames_available_total",
            "frames_requested_total",
            "late_fills",
            "max_lateness_ms",
            "min_padding_frames",
            "render_thread_priority",
        ] {
            assert_eq!(fields[key], Value::Null, "unmeasured {key} must be null");
        }
        assert_eq!(fields["duration_expected_ms"], json!(1_000.0));
    }

    #[test]
    fn unknown_expected_clip_length_is_null_not_zero() {
        let fields = clip_finished_fields("hero_kill", "finished", None, None, None);
        assert_eq!(fields["duration_expected_ms"], Value::Null);
    }

    #[test]
    fn log_fields_never_include_pack_paths_names_or_contents() {
        let fields = clip_finished_fields(
            "hero_kill",
            "finished",
            Some(48_000),
            Some(&measured_snapshot(48_000)),
            Some(0),
        );
        let serialized = serde_json::to_string(&fields).expect("fields serialize");

        for forbidden in ["pack_name", "pack_path", "visual_url", "announcer.wav"] {
            assert!(!serialized.contains(forbidden));
        }
        assert_eq!(fields["event_id"], json!("hero_kill"));
    }

    #[test]
    fn expected_duration_is_frames_at_48khz() {
        assert_eq!(duration_ms(Some(24_000)), Some(500.0));
        assert_eq!(duration_ms(None), None);
    }

    #[test]
    fn device_open_fields_keep_known_mix_request_and_null_unknown_endpoint_format() {
        let fields = device_open_fields(None);

        assert_eq!(fields["stream_sample_rate_hz"], json!(48_000));
        assert_eq!(fields["stream_channels"], json!(1));
        assert_eq!(fields["stream_bits_per_sample"], json!(16));
        assert_eq!(fields["autoconvert"], json!(true));
        for key in [
            "device_mix_sample_rate_hz",
            "device_mix_channels",
            "device_mix_bits_per_sample",
            "default_period_ms",
            "min_period_ms",
            "buffer_frames",
            "buffer_ms",
            "buffer_requested_ms",
            "buffer_fallback",
            "mmcss_task_index",
            "mmcss_ok",
            "mmcss_error_code",
            "priority_fallback_error_code",
            "render_thread_priority",
        ] {
            assert_eq!(fields[key], Value::Null, "unmeasured {key} must be null");
        }
    }

    #[test]
    fn device_open_fields_accept_mix_format_and_period_values() {
        let native = json!({
            "device_mix_sample_rate_hz": 44_100,
            "device_mix_channels": 2,
            "device_mix_bits_per_sample": 32,
            "default_period_ms": 10.0,
            "min_period_ms": 3.0,
            "buffer_frames": 480,
            "buffer_ms": 10.0,
            "buffer_requested_ms": 40.0,
            "buffer_fallback": false,
            "mmcss_task_index": 7,
            "mmcss_ok": true,
            "mmcss_error_code": null,
            "priority_fallback_error_code": null,
            "render_thread_priority": 0,
        })
        .to_string();
        let fields = device_open_fields(Some(&native));

        assert_eq!(fields["device_mix_sample_rate_hz"], json!(44_100));
        assert_eq!(fields["device_mix_channels"], json!(2));
        assert_eq!(fields["device_mix_bits_per_sample"], json!(32));
        assert_eq!(fields["default_period_ms"], json!(10.0));
        assert_eq!(fields["buffer_frames"], json!(480));
        assert_eq!(fields["buffer_requested_ms"], json!(40.0));
        assert_eq!(fields["buffer_fallback"], json!(false));
        assert_eq!(fields["mmcss_task_index"], json!(7));
        assert_eq!(fields["mmcss_ok"], json!(true));
        assert_eq!(fields["render_thread_priority"], json!(0));
    }

    #[test]
    fn pending_snapshot_marks_pending_and_never_emits_stale_write_counts() {
        let snapshot = json!({
            "pcm_frames": 960_u64,
            "cursor_frames": 960_u64,
            "frames_rendered": 480_u64,
            "frames_written": 480_u64,
            "write_error_count": 0_u64,
            "event_timeout_count": 2_u64,
            "snapshot_pending": true,
            "rendered_duration_known": false,
        })
        .to_string();
        let fields =
            clip_finished_fields("hero_kill", "finished", Some(960), Some(&snapshot), None);

        assert_eq!(fields["pcm_frames"], json!(960));
        assert_eq!(fields["cursor_frames"], json!(960));
        assert_eq!(fields["frames_written"], Value::Null);
        assert_eq!(fields["duration_rendered_ms"], Value::Null);
        assert_eq!(fields["write_error_count"], Value::Null);
        assert_eq!(fields["event_timeout_count"], json!(2));
        assert_eq!(fields["snapshot_pending"], json!(true));
    }
}
