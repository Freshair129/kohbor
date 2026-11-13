mod common;
use common::*;
use kohbor_capture::InvalidationReason;
use kohbor_vision::*;

fn point() -> MapPoint {
    MapPoint {
        x_milli: 123,
        y_milli: 456,
    }
}
fn sighting() -> Sighting {
    Sighting {
        track: TrackId(1),
        position: point(),
        class: None,
    }
}
fn tracker() -> Tracker {
    Tracker::new(CertifiedClasses::none())
}
fn provenance() -> CatalogProvenance {
    CatalogProvenance {
        patch: Some("p1".into()),
        source_hash: Some("hash".into()),
        coverage: CoverageStatus::Verified,
    }
}
macro_rules! field {
    ($name:ident, $value:expr, $availability:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let f: Field<u32> = $value;
            assert_eq!(f.availability(), $availability);
            assert_eq!(f.value().copied(), $expected);
        }
    };
}
field!(
    data_005_field_unknown,
    Field::unknown(),
    Availability::Unknown,
    None
);
field!(
    data_005_field_invalid,
    Field::invalid(),
    Availability::Invalid,
    None
);
field!(
    data_005_field_unsupported,
    Field::unsupported(),
    Availability::Unsupported,
    None
);
field!(
    data_005_field_fresh,
    Field::fresh(42),
    Availability::Fresh,
    Some(42)
);
field!(
    data_005_field_stale,
    Field::stale(42),
    Availability::Stale,
    Some(42)
);
field!(
    data_005_field_unknown_not_zero,
    Field::unknown(),
    Availability::Unknown,
    None::<u32>
);
field!(
    data_005_field_real_zero_fresh,
    Field::fresh(0),
    Availability::Fresh,
    Some(0)
);
field!(
    data_005_field_real_zero_stale,
    Field::stale(0),
    Availability::Stale,
    Some(0)
);
#[test]
fn data_005_observation_unobserved_track() {
    let o = EnemyObservation::unobserved(TrackId(8));
    assert_eq!(o.track, TrackId(8));
}
#[test]
fn data_005_observation_unobserved_position() {
    let o = EnemyObservation::unobserved(TrackId(8));
    assert_eq!(o.position.availability(), Availability::Unknown);
    assert_eq!(o.position.value(), None);
}
#[test]
fn data_005_observation_unobserved_identity() {
    assert_eq!(
        EnemyObservation::unobserved(TrackId(8)).identity,
        IdentityState::Unknown
    );
}
#[test]
fn data_005_observation_unobserved_presence() {
    assert_eq!(
        EnemyObservation::unobserved(TrackId(8)).presence,
        Presence::NeverSeen
    );
}
#[test]
fn data_005_observation_unobserved_last_seen() {
    assert_eq!(EnemyObservation::unobserved(TrackId(8)).last_seen_ms, None);
}
#[test]
fn data_005_observation_exhaustive_fields() {
    let EnemyObservation {
        track,
        position,
        identity,
        last_seen_ms,
        presence,
    } = EnemyObservation::unobserved(TrackId(8));
    assert_eq!(track, TrackId(8));
    assert_eq!(position.value(), None);
    assert_eq!(identity, IdentityState::Unknown);
    assert_eq!(last_seen_ms, None);
    assert_eq!(presence, Presence::NeverSeen);
}
#[test]
fn data_005_observation_never_sighted() {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 100), &[]);
    assert_eq!(
        t.observation(TrackId(1)),
        EnemyObservation::unobserved(TrackId(1))
    );
}
#[test]
fn data_005_observation_sighting_fresh_exact_point() {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 100), &[sighting()]);
    let o = t.observation(TrackId(1));
    assert_eq!(o.position.availability(), Availability::Fresh);
    assert_eq!(o.position.value(), Some(&point()));
}
#[test]
fn data_005_observation_sighting_visible_and_time() {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 100), &[sighting()]);
    let o = t.observation(TrackId(1));
    assert_eq!(o.presence, Presence::Visible);
    assert_eq!(o.last_seen_ms, Some(100));
}
#[test]
fn data_005_observation_next_frame_stale_without_drift() {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 100), &[sighting()]);
    t.on_admitted_frame(&frame(&mut c, 1, 2, 200), &[]);
    let o = t.observation(TrackId(1));
    assert_eq!(o.position.availability(), Availability::Stale);
    assert_eq!(o.position.value(), Some(&point()));
    assert_eq!(o.last_seen_ms, Some(100));
}
#[test]
fn vision_003_identity_none_is_empty() {
    assert_eq!(CertifiedClasses::none().len(), 0);
}
#[test]
fn vision_003_identity_none_does_not_contain_class() {
    assert!(!CertifiedClasses::none().contains(&ClassId("a".into())));
}
#[test]
fn vision_003_identity_unverified_a() {
    assert_eq!(
        classify(Some(&ClassId("a".into())), &CertifiedClasses::none()),
        IdentityState::Unverified
    );
}
#[test]
fn vision_003_identity_unverified_b() {
    assert_eq!(
        classify(Some(&ClassId("b".into())), &CertifiedClasses::none()),
        IdentityState::Unverified
    );
}
#[test]
fn vision_003_identity_unverified_has_no_payload() {
    match classify(Some(&ClassId("a".into())), &CertifiedClasses::none()) {
        IdentityState::Unverified => (),
        _ => panic!("expected unverified"),
    }
}
#[test]
fn vision_003_identity_certified_class() {
    let id = ClassId("a".into());
    let set = CertifiedClasses::from_provenance(&provenance(), &[id.clone()]);
    assert_eq!(classify(Some(&id), &set), IdentityState::Certified(id));
}
#[test]
fn vision_003_identity_no_class_unknown() {
    assert_eq!(
        classify(None, &CertifiedClasses::none()),
        IdentityState::Unknown
    );
}
#[test]
fn vision_003_identity_partial_empty() {
    let mut p = provenance();
    p.coverage = CoverageStatus::Partial;
    assert_eq!(
        CertifiedClasses::from_provenance(&p, &[ClassId("a".into())]).len(),
        0
    );
}
#[test]
fn vision_003_identity_unsupported_empty() {
    let mut p = provenance();
    p.coverage = CoverageStatus::Unsupported;
    assert_eq!(
        CertifiedClasses::from_provenance(&p, &[ClassId("a".into())]).len(),
        0
    );
}
#[test]
fn vision_003_identity_missing_hash_empty() {
    let mut p = provenance();
    p.source_hash = None;
    assert_eq!(
        CertifiedClasses::from_provenance(&p, &[ClassId("a".into())]).len(),
        0
    );
}
#[test]
fn vision_002_coverage_never_seen_never_absent() {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 0), &[]);
    t.on_admitted_frame(&frame(&mut c, 1, 2, 2000), &[]);
    assert_eq!(t.observation(TrackId(1)).presence, Presence::NeverSeen);
    assert!(t.absent_beyond(0).is_empty());
}
fn seen() -> (kohbor_capture::CaptureController, Tracker) {
    let mut c = controller(1);
    let mut t = tracker();
    t.on_admitted_frame(&frame(&mut c, 1, 1, 100), &[sighting()]);
    (c, t)
}
#[test]
fn vision_002_coverage_first_absence_accrues() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 1100), &[]);
    assert_eq!(
        t.observation(TrackId(1)).presence,
        Presence::Absent { covered_ms: 1000 }
    );
}
#[test]
fn vision_002_coverage_consecutive_accrues() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 1100), &[]);
    t.on_admitted_frame(&frame(&mut c, 1, 3, 3100), &[]);
    assert_eq!(
        t.observation(TrackId(1)).presence,
        Presence::Absent { covered_ms: 3000 }
    );
}
#[test]
fn vision_002_coverage_exact_2000_accrues() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 2100), &[]);
    assert_eq!(
        t.observation(TrackId(1)).presence,
        Presence::Absent { covered_ms: 2000 }
    );
}
#[test]
fn vision_002_coverage_gap_2001_interrupts() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 2101), &[]);
    assert_eq!(t.observation(TrackId(1)).presence, Presence::Interrupted);
}
#[test]
fn vision_002_coverage_gap_discards_previous_accrual() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 1100), &[]);
    t.on_admitted_frame(&frame(&mut c, 1, 3, 3101), &[]);
    t.on_admitted_frame(&frame(&mut c, 1, 4, 3201), &[]);
    assert_eq!(
        t.observation(TrackId(1)).presence,
        Presence::Absent { covered_ms: 100 }
    );
}
#[test]
fn vision_002_coverage_lost_interrupts() {
    let (_, mut t) = seen();
    t.on_coverage_lost(101);
    assert_eq!(t.observation(TrackId(1)).presence, Presence::Interrupted);
}
#[test]
fn vision_002_coverage_lost_keeps_stale_point() {
    let (_, mut t) = seen();
    t.on_coverage_lost(101);
    let o = t.observation(TrackId(1));
    assert_eq!(o.position.availability(), Availability::Stale);
    assert_eq!(o.position.value(), Some(&point()));
}
#[test]
fn vision_002_coverage_lost_first_frame_is_baseline() {
    let (mut c, mut t) = seen();
    t.on_coverage_lost(200);
    t.on_admitted_frame(&frame(&mut c, 1, 2, 300), &[]);
    assert!(!t.absent_beyond(0).contains(&TrackId(1)));
}
#[test]
fn vision_002_coverage_lost_then_accrues_from_baseline() {
    let (mut c, mut t) = seen();
    t.on_coverage_lost(200);
    t.on_admitted_frame(&frame(&mut c, 1, 2, 300), &[]);
    t.on_admitted_frame(&frame(&mut c, 1, 3, 400), &[]);
    assert_eq!(
        t.observation(TrackId(1)).presence,
        Presence::Absent { covered_ms: 100 }
    );
}
#[test]
fn vision_002_coverage_threshold_strict_equal() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 1100), &[]);
    assert!(t.absent_beyond(1000).is_empty());
}
#[test]
fn vision_002_coverage_threshold_one_greater() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 1101), &[]);
    assert_eq!(t.absent_beyond(1000), vec![TrackId(1)]);
}
#[test]
fn vision_002_coverage_interrupted_not_missing() {
    let (mut c, mut t) = seen();
    t.on_admitted_frame(&frame(&mut c, 1, 2, 2101), &[]);
    assert!(t.absent_beyond(0).is_empty());
    assert_eq!(t.observation(TrackId(1)).presence, Presence::Interrupted);
}
#[test]
fn vision_002_coverage_session_clears_tracks() {
    let (mut c, mut t) = seen();
    c.set_session_epoch(2);
    t.on_admitted_frame(&frame(&mut c, 2, 2, 200), &[]);
    assert!(t.tracks().is_empty());
    assert_eq!(t.observation(TrackId(1)).presence, Presence::NeverSeen);
}
#[test]
fn vision_002_coverage_generation_clears_tracks() {
    let (mut c, mut t) = seen();
    c.calibrator_mut()
        .invalidate(InvalidationReason::UserAction);
    let mut replacement = controller(1);
    replacement
        .calibrator_mut()
        .invalidate(InvalidationReason::UserAction);
    let env = kohbor_capture::Environment {
        output: kohbor_capture::OutputGeometry {
            output_id: 7,
            width: 100,
            height: 100,
            dpi: 96,
        },
        monitor_id: 2,
        window_rect: kohbor_capture::Rect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        },
        minimap_layout: "left".into(),
    };
    replacement
        .calibrator_mut()
        .begin(
            env,
            kohbor_capture::Rect {
                x: 0,
                y: 0,
                width: 20,
                height: 20,
            },
            None,
        )
        .unwrap();
    for _ in 0..30 {
        replacement.calibrator_mut().structural_frame(true);
    }
    replacement
        .calibrator_mut()
        .confirm(kohbor_capture::CalibrationMethod::Manual, 0)
        .unwrap();
    t.on_admitted_frame(&frame(&mut replacement, 1, 2, 200), &[]);
    assert!(t.tracks().is_empty());
    assert_eq!(t.observation(TrackId(1)).presence, Presence::NeverSeen);
}
macro_rules! label {
    ($name:ident, $edit:expr, $patch:expr, $expected:expr) => {
        #[test]
        fn $name() {
            let mut p = provenance();
            ($edit)(&mut p);
            assert_eq!(fact_label(&p, $patch), $expected);
        }
    };
}
label!(
    data_006_provenance_no_patch,
    |p: &mut CatalogProvenance| p.patch = None,
    "p1",
    FactLabel::Unsupported
);
label!(
    data_006_provenance_different_patch,
    |_: &mut CatalogProvenance| {},
    "p2",
    FactLabel::Unsupported
);
label!(
    data_006_provenance_partial,
    |p: &mut CatalogProvenance| p.coverage = CoverageStatus::Partial,
    "p1",
    FactLabel::Partial
);
label!(
    data_006_provenance_missing_hash,
    |p: &mut CatalogProvenance| p.source_hash = None,
    "p1",
    FactLabel::Partial
);
label!(
    data_006_provenance_verified,
    |_: &mut CatalogProvenance| {},
    "p1",
    FactLabel::Verified
);
label!(
    data_006_provenance_unsupported,
    |p: &mut CatalogProvenance| p.coverage = CoverageStatus::Unsupported,
    "p1",
    FactLabel::Unsupported
);
label!(
    data_006_provenance_wrong_patch_precedes_partial,
    |p: &mut CatalogProvenance| p.coverage = CoverageStatus::Partial,
    "p2",
    FactLabel::Unsupported
);
#[test]
fn data_006_provenance_labels_exhaustive_no_latest() {
    let label = fact_label(&provenance(), "p1");
    let name = match label {
        FactLabel::Verified => "verified",
        FactLabel::Partial => "partial",
        FactLabel::Unsupported => "unsupported",
    };
    assert_eq!(name, "verified");
}
