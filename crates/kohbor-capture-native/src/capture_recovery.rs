// Provenance: ported from owner-authored G-Maiden 0.13.6 src-tauri/src/capture_recovery.rs, owner consent recorded in claim vision-risk__mvp1-capture-native.

pub(crate) fn replace_released<T, E>(
    slot: &mut Option<T>,
    create: impl FnOnce() -> Result<T, E>,
) -> Result<(), E> {
    drop(slot.take());
    *slot = Some(create()?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    struct Exclusive(Rc<Cell<bool>>);

    impl Drop for Exclusive {
        fn drop(&mut self) {
            self.0.set(false);
        }
    }

    #[test]
    fn old_duplication_is_released_before_recreation() {
        let held = Rc::new(Cell::new(true));
        let mut slot = Some(Exclusive(held.clone()));
        replace_released(&mut slot, || {
            assert!(!held.get());
            held.set(true);
            Ok::<_, ()>(Exclusive(held.clone()))
        })
        .unwrap();
        assert!(held.get());
    }

    #[test]
    fn failed_recreation_leaves_no_stale_duplication() {
        let held = Rc::new(Cell::new(true));
        let mut slot = Some(Exclusive(held.clone()));
        assert!(replace_released(&mut slot, || Err::<Exclusive, _>("lost")).is_err());
        assert!(slot.is_none());
        assert!(!held.get());
    }
}
