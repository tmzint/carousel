use crate::platform::input::{InputEvent, Inputs, MouseButton, ScrollDirection};
use crate::platform::key::ScanCode;
use crate::platform::message::{ActionEvent, KeyInputEvent, MouseInputEvent, ScrollInputEvent};
use crate::some_or_continue;
use crate::util::{HashMap, IndexMap};
use indexmap::map::Entry;
use internment::Intern;
use roundabout::prelude::MessageSender;
use serde::{Deserialize, Serialize};
use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(tag = "input", content = "select", rename_all = "camelCase")]
pub enum ActionTrigger {
    Mouse(MouseButton),
    Scroll(ScrollDirection),
    Key(ScanCode),
}

#[derive(Default, Debug, Clone, Deserialize)]
#[serde(
    from = "Vec<(ActionTrigger, Intern<String>)>",
    into = "Vec<(ActionTrigger, Intern<String>)>"
)]
pub struct ActionsConfig(HashMap<ActionTrigger, Intern<String>>);

impl Deref for ActionsConfig {
    type Target = HashMap<ActionTrigger, Intern<String>>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ActionsConfig {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<Vec<(ActionTrigger, Intern<String>)>> for ActionsConfig {
    #[inline]
    fn from(vectorized: Vec<(ActionTrigger, Intern<String>)>) -> Self {
        Self(HashMap::from_iter(vectorized.into_iter()))
    }
}

impl Into<Vec<(ActionTrigger, Intern<String>)>> for ActionsConfig {
    #[inline]
    fn into(self) -> Vec<(ActionTrigger, Intern<String>)> {
        self.0.into_iter().collect()
    }
}

#[derive(Debug)]
struct CurrentActionState {
    state: ActionState,
    value: f32,
    tick: u64,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ActionState {
    Start,
    Hold,
    End,
}

#[derive(Debug, Default)]
pub struct Actions {
    config: ActionsConfig,
    current: IndexMap<Intern<String>, CurrentActionState>,
    buffer: Vec<ActionEvent>,
    tick: u64,
    trigger_value_cache: Vec<(ActionTrigger, f32)>,
}

impl Actions {
    pub(crate) fn new(config: ActionsConfig) -> Self {
        Self {
            config,
            ..Default::default()
        }
    }

    pub(crate) fn set_config(&mut self, config: ActionsConfig) {
        for (action, _) in self.current.drain(..) {
            self.buffer.push(ActionEvent {
                name: action,
                state: ActionState::End,
                value: 0.0,
            });
        }

        self.config = config;
    }

    pub(crate) fn push_inputs(&mut self, inputs: &Inputs) {
        // TODO:
        //  this allows multiple actions to be fired per frame, do we want this?
        //  Start -> Release -> Start -> ...
        for input in inputs.queued_events() {
            match input {
                InputEvent::Mouse(MouseInputEvent { button, value }) => {
                    self.trigger_value_cache
                        .push((ActionTrigger::Mouse(*button), *value));
                }
                InputEvent::Scroll(ScrollInputEvent { direction, value }) => {
                    self.trigger_value_cache
                        .push((ActionTrigger::Scroll(*direction), *value));
                    self.trigger_value_cache
                        .push((ActionTrigger::Scroll(*direction), 0.0));
                }
                InputEvent::Key(KeyInputEvent { scan, value }) => {
                    self.trigger_value_cache
                        .push((ActionTrigger::Key(*scan), *value));
                }
                InputEvent::Cursor(_) => {
                    continue;
                }
            };

            for (trigger, value) in self.trigger_value_cache.drain(..) {
                let action = *some_or_continue!(self.config.get(&trigger));

                let is_end = value.abs() <= f32::EPSILON;
                if is_end {
                    self.current.remove(&action);
                    self.buffer.push(ActionEvent {
                        name: action,
                        state: ActionState::End,
                        value: 0.0,
                    });

                    continue;
                }

                match self.current.entry(action) {
                    Entry::Occupied(mut entry) => {
                        entry.get_mut().value = value;
                    }
                    Entry::Vacant(entry) => {
                        entry.insert(CurrentActionState {
                            state: ActionState::Start,
                            value,
                            tick: self.tick,
                        });

                        self.buffer.push(ActionEvent {
                            name: action,
                            state: ActionState::Start,
                            value,
                        });
                    }
                }
            }
        }
    }

    pub(crate) fn apply_actions(&mut self, sender: &MessageSender) {
        for (action, state) in &mut self.current {
            state.state = ActionState::Hold;

            if state.tick < self.tick {
                self.buffer.push(ActionEvent {
                    name: *action,
                    state: ActionState::Hold,
                    value: state.value,
                });
            }
        }

        sender.send_iter(self.buffer.drain(..));
        self.tick += 1;
    }
}
