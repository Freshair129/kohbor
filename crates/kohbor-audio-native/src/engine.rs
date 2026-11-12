use kohbor_audio::{PlaybackId, SinkError, SinkEvent};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TryRecvError, TrySendError};
use std::sync::Arc;

pub(super) const COMMAND_QUEUE_CAPACITY: usize = 8;
pub(super) const EVENT_QUEUE_CAPACITY: usize = 16;
const RETIREMENT_QUEUE_CAPACITY: usize = COMMAND_QUEUE_CAPACITY + 1;
const INACTIVE_GENERATION: u64 = 0;

enum Command {
    Submit {
        id: PlaybackId,
        generation: u64,
        samples: Arc<[i16]>,
    },
}

struct Playback {
    id: PlaybackId,
    generation: u64,
    samples: Arc<[i16]>,
    cursor: usize,
    finished_notified: bool,
}

/// Device-independent render state. `fill` is called only on the audio-owner
/// thread and uses fixed-capacity channels plus the current buffer; it does not
/// wait for the coordinator.
pub(crate) struct Engine {
    commands: Receiver<Command>,
    retired: SyncSender<Arc<[i16]>>,
    events: SyncSender<SinkEvent>,
    current_generation: Arc<AtomicU64>,
    active: Option<Playback>,
    pending_command: Option<Command>,
}

/// Coordinator-side half of the bounded hand-off. Callers are serialized by
/// `&mut self`; render-side cancellation observes only the atomic generation.
pub(crate) struct EngineHandle {
    commands: SyncSender<Command>,
    retired: Receiver<Arc<[i16]>>,
    event_sender: SyncSender<SinkEvent>,
    events: Receiver<SinkEvent>,
    current_generation: Arc<AtomicU64>,
    device_lost_pending: Arc<AtomicBool>,
    device_stop_requested: Arc<AtomicBool>,
    next_generation: u64,
    current_id: Option<PlaybackId>,
}

/// Compact device notifications from WASAPI. Device loss has its own one-slot
/// atomic flag and is returned before queued noncritical events. When the event
/// channel is full, the newest `Finished` or `DeviceChanged` event is dropped.
pub(crate) struct DeviceEventSender {
    events: SyncSender<SinkEvent>,
    device_lost_pending: Arc<AtomicBool>,
    device_stop_requested: Arc<AtomicBool>,
}

impl DeviceEventSender {
    pub(crate) fn clone_for_callback(&self) -> Self {
        Self {
            events: Clone::clone(&self.events),
            device_lost_pending: Arc::clone(&self.device_lost_pending),
            device_stop_requested: Arc::clone(&self.device_stop_requested),
        }
    }

    pub(crate) fn device_lost(&self) {
        self.device_stop_requested.store(true, Ordering::Release);
        self.device_lost_pending.store(true, Ordering::Release);
    }

    pub(crate) fn device_changed(&self) {
        let _ = self.events.try_send(SinkEvent::DeviceChanged);
    }

    pub(crate) fn stop_requested(&self) -> bool {
        self.device_stop_requested.load(Ordering::Acquire)
    }
}

impl Engine {
    pub(crate) fn new() -> (Self, EngineHandle) {
        let (command_sender, commands) = mpsc::sync_channel(COMMAND_QUEUE_CAPACITY);
        let (retired_sender, retired) = mpsc::sync_channel(RETIREMENT_QUEUE_CAPACITY);
        let (event_sender, events) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let current_generation = Arc::new(AtomicU64::new(INACTIVE_GENERATION));
        let device_lost_pending = Arc::new(AtomicBool::new(false));
        let device_stop_requested = Arc::new(AtomicBool::new(false));
        let handle = EngineHandle {
            commands: command_sender,
            retired,
            event_sender: Clone::clone(&event_sender),
            events,
            current_generation: Arc::clone(&current_generation),
            device_lost_pending: Arc::clone(&device_lost_pending),
            device_stop_requested: Arc::clone(&device_stop_requested),
            next_generation: INACTIVE_GENERATION,
            current_id: None,
        };
        (
            Self {
                commands,
                retired: retired_sender,
                events: event_sender,
                current_generation,
                active: None,
                pending_command: None,
            },
            handle,
        )
    }

    /// Fill the render block with the active samples, padding the final block or
    /// an idle/cancelled slot with silence.
    pub(crate) fn fill(&mut self, out: &mut [i16]) {
        out.fill(0);
        if !self.process_commands() {
            return;
        }
        self.render_active(out);
        self.retire_terminal_active();
    }

    fn process_commands(&mut self) -> bool {
        if !self.retire_terminal_active() {
            return false;
        }

        for _ in 0..COMMAND_QUEUE_CAPACITY {
            let command = match self.pending_command.take() {
                Some(command) => command,
                None => match self.commands.try_recv() {
                    Ok(command) => command,
                    Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
                },
            };

            if !self.process_command(command) {
                return false;
            }
        }
        true
    }

    fn process_command(&mut self, command: Command) -> bool {
        let Command::Submit {
            id,
            generation,
            samples,
        } = command;
        if self.current_generation.load(Ordering::Acquire) != generation {
            return match self.retired.try_send(samples) {
                Ok(()) => true,
                Err(TrySendError::Full(samples) | TrySendError::Disconnected(samples)) => {
                    self.pending_command = Some(Command::Submit {
                        id,
                        generation,
                        samples,
                    });
                    false
                }
            };
        }

        if self.active.is_some() && !self.retire_active() {
            self.pending_command = Some(Command::Submit {
                id,
                generation,
                samples,
            });
            return false;
        }

        self.active = Some(Playback {
            id,
            generation,
            samples,
            cursor: 0,
            finished_notified: false,
        });
        true
    }

    fn render_active(&mut self, out: &mut [i16]) {
        let generation = self.current_generation.load(Ordering::Acquire);
        let Some(active) = self.active.as_mut() else {
            return;
        };
        if generation != active.generation {
            return;
        }

        let remaining = active.samples.len().saturating_sub(active.cursor);
        let count = remaining.min(out.len());
        let Some(end) = active.cursor.checked_add(count) else {
            return;
        };
        out[..count].copy_from_slice(&active.samples[active.cursor..end]);
        if self.current_generation.load(Ordering::Acquire) != generation {
            out.fill(0);
            return;
        }
        active.cursor = end;
        if active.cursor == active.samples.len() && !active.finished_notified {
            active.finished_notified = true;
            let id = active.id;
            let _ = self.events.try_send(SinkEvent::Finished { id });
            let _ = self.current_generation.compare_exchange(
                generation,
                INACTIVE_GENERATION,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        }
    }

    fn retire_terminal_active(&mut self) -> bool {
        let should_retire = self.active.as_ref().is_some_and(|active| {
            active.finished_notified
                || active.generation != self.current_generation.load(Ordering::Acquire)
        });
        !should_retire || self.retire_active()
    }

    fn retire_active(&mut self) -> bool {
        let Some(active) = self.active.take() else {
            return true;
        };
        let Playback {
            id,
            generation,
            samples,
            cursor,
            finished_notified,
        } = active;
        match self.retired.try_send(samples) {
            Ok(()) => true,
            Err(TrySendError::Full(samples) | TrySendError::Disconnected(samples)) => {
                self.active = Some(Playback {
                    id,
                    generation,
                    samples,
                    cursor,
                    finished_notified,
                });
                false
            }
        }
    }
}

impl EngineHandle {
    pub(crate) fn submit(&mut self, id: PlaybackId, samples: Arc<[i16]>) -> Result<(), SinkError> {
        self.reclaim_retired();
        let Some(generation) = self.next_generation.checked_add(1) else {
            self.current_generation
                .store(INACTIVE_GENERATION, Ordering::Release);
            self.current_id = None;
            return Err(SinkError);
        };
        self.next_generation = generation;
        self.current_id = Some(id);
        self.current_generation.store(generation, Ordering::Release);
        let command = Command::Submit {
            id,
            generation,
            samples,
        };
        match self.commands.try_send(command) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(command) | TrySendError::Disconnected(command)) => {
                self.current_generation
                    .compare_exchange(
                        generation,
                        INACTIVE_GENERATION,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .ok();
                self.current_id = None;
                drop(command);
                Err(SinkError)
            }
        }
    }

    pub(crate) fn cancel(&mut self, id: PlaybackId) {
        if self.current_id == Some(id) {
            self.current_generation
                .store(INACTIVE_GENERATION, Ordering::Release);
            self.current_id = None;
        }
    }

    pub(crate) fn poll_event(&mut self) -> Option<SinkEvent> {
        self.reclaim_retired();
        if self.device_lost_pending.swap(false, Ordering::AcqRel) {
            return Some(SinkEvent::DeviceLost);
        }
        self.events.try_recv().ok()
    }

    pub(crate) fn poll_retired(&mut self) -> Option<Arc<[i16]>> {
        self.retired.try_recv().ok()
    }

    pub(crate) fn device_event_sender(&self) -> DeviceEventSender {
        DeviceEventSender {
            events: Clone::clone(&self.event_sender),
            device_lost_pending: Arc::clone(&self.device_lost_pending),
            device_stop_requested: Arc::clone(&self.device_stop_requested),
        }
    }

    fn reclaim_retired(&mut self) {
        while let Some(samples) = self.poll_retired() {
            drop(samples);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Engine, COMMAND_QUEUE_CAPACITY, EVENT_QUEUE_CAPACITY};
    use kohbor_audio::{PlaybackId, SinkError, SinkEvent};
    use std::sync::Arc;

    fn pair() -> (Engine, super::EngineHandle) {
        Engine::new()
    }

    fn samples(values: &[i16]) -> Arc<[i16]> {
        Arc::from(values.to_vec())
    }

    fn submit(engine: &mut super::EngineHandle, id: u64, values: &[i16]) {
        assert_eq!(engine.submit(PlaybackId(id), samples(values)), Ok(()));
    }

    #[test]
    fn a_new_submit_replaces_the_current_buffer_without_mixing() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[1, 2, 3, 4]);
        let mut first = [0; 2];
        render.fill(&mut first);
        assert_eq!(first, [1, 2]);

        submit(&mut coordinator, 2, &[7, 8]);
        let mut replacement = [0; 2];
        render.fill(&mut replacement);

        assert_eq!(replacement, [7, 8]);
        assert_eq!(
            coordinator.poll_retired().as_deref(),
            Some(&[1, 2, 3, 4][..])
        );
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(2) })
        );
    }

    #[test]
    fn cancel_makes_the_next_fill_silent_and_retires_without_finished() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[9, 8, 7]);
        coordinator.cancel(PlaybackId(1));

        let mut output = [4; 5];
        render.fill(&mut output);

        assert_eq!(output, [0; 5]);
        assert_eq!(coordinator.poll_retired().as_deref(), Some(&[9, 8, 7][..]));
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn cancelling_a_stale_id_does_not_cancel_the_replacement() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[1, 2, 3]);
        let mut prefix = [0; 1];
        render.fill(&mut prefix);
        submit(&mut coordinator, 2, &[6, 5, 4]);
        coordinator.cancel(PlaybackId(1));

        let mut output = [0; 3];
        render.fill(&mut output);

        assert_eq!(output, [6, 5, 4]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(2) })
        );
    }

    #[test]
    fn completion_is_emitted_exactly_once_after_the_last_sample() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 17, &[3, 2, 1]);
        let mut first = [0; 2];
        render.fill(&mut first);
        assert_eq!(first, [3, 2]);
        assert_eq!(coordinator.poll_event(), None);

        let mut last = [9; 2];
        render.fill(&mut last);
        assert_eq!(last, [1, 0]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(17) })
        );
        render.fill(&mut last);
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn zero_length_fill_is_safe_and_does_not_consume_samples() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[5, 4]);
        render.fill(&mut []);

        let mut output = [0; 2];
        render.fill(&mut output);

        assert_eq!(output, [5, 4]);
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(1) })
        );
    }

    #[test]
    fn a_short_final_buffer_is_padded_with_silence_and_retired() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[11, 12, 13]);
        let mut output = [0; 5];

        render.fill(&mut output);

        assert_eq!(output, [11, 12, 13, 0, 0]);
        assert_eq!(
            coordinator.poll_retired().as_deref(),
            Some(&[11, 12, 13][..])
        );
        assert_eq!(
            coordinator.poll_event(),
            Some(SinkEvent::Finished { id: PlaybackId(1) })
        );
    }

    #[test]
    fn finished_buffers_are_returned_to_the_coordinator() {
        let (mut render, mut coordinator) = pair();
        submit(&mut coordinator, 1, &[21, 22]);
        render.fill(&mut [0; 2]);

        let retired = coordinator.poll_retired();
        assert_eq!(retired.as_deref(), Some(&[21, 22][..]));
    }

    #[test]
    fn a_full_command_queue_returns_sink_error_and_leaves_silence() {
        let (mut render, mut coordinator) = pair();
        for id in 1..=COMMAND_QUEUE_CAPACITY as u64 {
            submit(&mut coordinator, id, &[id as i16]);
        }

        assert_eq!(
            coordinator.submit(PlaybackId(99), samples(&[99])),
            Err(SinkError)
        );
        let mut output = [7; 1];
        render.fill(&mut output);
        assert_eq!(output, [0]);
        let mut retired = 0;
        while coordinator.poll_retired().is_some() {
            retired += 1;
        }
        assert_eq!(retired, COMMAND_QUEUE_CAPACITY);
    }

    #[test]
    fn a_full_event_queue_drops_newest_but_preserves_device_lost() {
        let (mut render, mut coordinator) = pair();
        for id in 1..=EVENT_QUEUE_CAPACITY as u64 + 1 {
            submit(&mut coordinator, id, &[id as i16]);
            render.fill(&mut [0; 1]);
        }
        coordinator.device_event_sender().device_changed();
        coordinator.device_event_sender().device_lost();

        assert_eq!(coordinator.poll_event(), Some(SinkEvent::DeviceLost));
        for id in 1..=EVENT_QUEUE_CAPACITY as u64 {
            assert_eq!(
                coordinator.poll_event(),
                Some(SinkEvent::Finished { id: PlaybackId(id) })
            );
        }
        assert_eq!(coordinator.poll_event(), None);
    }

    #[test]
    fn device_changed_is_queued_as_a_bounded_event() {
        let (render, mut coordinator) = pair();
        coordinator.device_event_sender().device_changed();
        drop(render);

        assert_eq!(coordinator.poll_event(), Some(SinkEvent::DeviceChanged));
    }

    #[test]
    fn deterministic_randomish_sequences_remain_consistent() {
        let (mut render, mut coordinator) = pair();
        let mut seed = 0x6d2b_79f5_u32;
        let mut next_id = 1_u64;
        let mut current: Option<(PlaybackId, Vec<i16>, usize)> = None;
        let mut generation_active = true;

        for _ in 0..10_000 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            match seed % 4 {
                0 => {
                    let count = ((seed >> 8) % 5 + 1) as usize;
                    let values: Vec<i16> = (0..count)
                        .map(|index| (next_id as i16).wrapping_add(index as i16))
                        .collect();
                    let id = PlaybackId(next_id);
                    next_id += 1;
                    assert_eq!(coordinator.submit(id, samples(&values)), Ok(()));
                    current = Some((id, values, 0));
                    generation_active = true;
                }
                1 => {
                    if let Some((id, _, _)) = current.as_ref() {
                        let cancel_id = if seed & 0x1000 == 0 {
                            *id
                        } else {
                            PlaybackId(id.0.saturating_add(1_000_000))
                        };
                        coordinator.cancel(cancel_id);
                        if cancel_id == *id {
                            generation_active = false;
                        }
                    } else {
                        coordinator.cancel(PlaybackId(seed as u64));
                    }
                }
                _ => {}
            }

            let output_len = ((seed >> 16) % 7) as usize;
            let mut actual = vec![99_i16; output_len];
            let mut expected = vec![0_i16; output_len];
            let mut clear_current = false;
            let mut expected_finished = None;
            if let Some((id, values, cursor)) = current.as_mut() {
                if !generation_active {
                    clear_current = true;
                } else {
                    let count = output_len.min(values.len().saturating_sub(*cursor));
                    expected[..count].copy_from_slice(&values[*cursor..*cursor + count]);
                    *cursor += count;
                    if *cursor == values.len() {
                        clear_current = true;
                        expected_finished = Some(*id);
                    }
                }
            }
            if clear_current {
                current = None;
            }
            render.fill(&mut actual);
            assert_eq!(actual, expected);
            if let Some(id) = expected_finished {
                assert_eq!(coordinator.poll_event(), Some(SinkEvent::Finished { id }));
            }
            while coordinator.poll_retired().is_some() {}
            while let Some(event) = coordinator.poll_event() {
                assert!(!matches!(event, SinkEvent::Finished { .. }));
            }
        }
    }

    #[test]
    fn render_source_guard_rejects_blocking_allocating_or_io_primitives() {
        let source = include_str!("engine.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or(source);
        let code_without_comments = strip_comments(production);

        for forbidden in [
            "Mutex",
            "RwLock",
            "Vec::new",
            "Box::new",
            "String",
            "format!",
            ".clone()",
            "println!",
            "std::fs",
            "std::net",
            "TcpStream",
            "UdpSocket",
        ] {
            assert!(
                !code_without_comments.contains(forbidden),
                "engine render source contains forbidden token {forbidden}"
            );
        }
    }

    fn strip_comments(source: &str) -> String {
        let mut output = String::with_capacity(source.len());
        let mut characters = source.chars().peekable();
        let mut block_depth = 0_usize;
        while let Some(character) = characters.next() {
            if block_depth > 0 {
                if character == '/' && characters.peek() == Some(&'*') {
                    characters.next();
                    block_depth += 1;
                } else if character == '*' && characters.peek() == Some(&'/') {
                    characters.next();
                    block_depth -= 1;
                } else if character == '\n' {
                    output.push('\n');
                }
            } else if character == '/' && characters.peek() == Some(&'/') {
                characters.next();
                for comment_character in characters.by_ref() {
                    if comment_character == '\n' {
                        output.push('\n');
                        break;
                    }
                }
            } else if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                block_depth = 1;
            } else {
                output.push(character);
            }
        }
        output
    }
}
