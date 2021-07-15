use crate::platform::message::FrameRequestedEvent;
use roundabout::prelude::*;
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Eq, PartialEq, Hash, Copy, Clone)]
struct ScheduleKey(Duration, Uuid);

impl PartialOrd for ScheduleKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ScheduleKey {
    fn cmp(&self, other: &Self) -> Ordering {
        other.0.cmp(&self.0).then(self.1.cmp(&other.1))
    }
}

pub struct TimeServer {
    elapsed: Duration,
    scheduled: BTreeMap<ScheduleKey, UntypedMessage>,
}

impl TimeServer {
    pub fn new(
        handler: OpenMessageHandlerBuilder<TimeServer>,
    ) -> InitMessageHandlerBuilder<TimeServer> {
        handler
            .on(on_frame_requested_event)
            .on(on_schedule_timer_event)
            .init(TimeServer {
                elapsed: Default::default(),
                scheduled: Default::default(),
            })
    }

    pub fn schedule<E: 'static + Send + Sync>(
        duration: Duration,
        event: E,
        sender: &MessageSender,
    ) {
        let sender = sender.borrow();
        match sender.prepare(event) {
            Some(scheduled) => {
                sender.send(ScheduleTimerEvent {
                    id: Uuid::new_v4(),
                    duration,
                    scheduled: Mutex::new(Some(scheduled)),
                });
            }
            None => {
                log::warn!(
                    "skipping scheduling of timer for unhandled event type: {}",
                    std::any::type_name::<E>()
                );
            }
        }
    }
}

fn on_frame_requested_event(
    state: &mut TimeServer,
    context: &mut RuntimeContext,
    event: &FrameRequestedEvent,
) {
    state.elapsed = event.elapsed;

    // Optimization: batching | append vs prepend
    while let Some(entry) = state.scheduled.last_entry() {
        if entry.key().0 > state.elapsed {
            break;
        }

        log::debug!("trigger timer event: {}", entry.key().1);
        context.sender().send_untyped(entry.remove());
    }
}

fn on_schedule_timer_event(
    state: &mut TimeServer,
    _context: &mut RuntimeContext,
    event: &ScheduleTimerEvent,
) {
    let at = state.elapsed + event.duration;
    log::debug!("schedule timer event: {}", event.id);
    // Optimization: batching | append vs prepend
    state.scheduled.insert(
        ScheduleKey(at, event.id),
        event.scheduled.lock().unwrap().take().unwrap(),
    );
}

pub struct ScheduleTimerEvent {
    id: Uuid,
    duration: Duration,
    scheduled: Mutex<Option<UntypedMessage>>,
}
