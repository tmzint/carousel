use crate::asset::storage::Assets;
use crate::asset::AssetsCreatedEvent;
use crate::platform::message::FrameRequestedEvent;
use crate::render::client::RenderClient;
use crate::render::message::RenderCreatedEvent;
use crate::some_or_return;
use roundabout::prelude::*;
use std::marker::PhantomData;

pub enum StateInstruction<T> {
    Stay,
    Switch(T),
    Push(Vec<T>),
    Pop,
    PopPush(Vec<T>),
    Extract,
}

impl<T> StateInstruction<T> {
    #[inline]
    pub fn switch<I: Into<T>>(state: I) -> Self {
        StateInstruction::Switch(state.into())
    }

    #[inline]
    pub fn push<I: Into<T>>(state: I) -> Self {
        StateInstruction::Push(vec![state.into()])
    }

    #[inline]
    pub fn push_iter<I: IntoIterator<Item = T>>(state: I) -> Self {
        StateInstruction::Push(state.into_iter().collect())
    }

    #[inline]
    pub fn pop_push<I: Into<T>>(state: I) -> Self {
        StateInstruction::PopPush(vec![state.into()])
    }

    #[inline]
    pub fn pop_push_iter<I: IntoIterator<Item = T>>(state: I) -> Self {
        StateInstruction::PopPush(state.into_iter().collect())
    }

    #[inline]
    pub fn is_stay(&self) -> bool {
        if let StateInstruction::Stay = self {
            true
        } else {
            false
        }
    }
}

pub struct SimResources<T> {
    pub context: RuntimeContext,
    pub assets: Assets,
    pub render: RenderClient,
    pub resource: T,
}

impl<T> AsRef<RuntimeContext> for SimResources<T> {
    #[inline]
    fn as_ref(&self) -> &RuntimeContext {
        &self.context
    }
}

pub trait SimState<R, S: Sized = Self>
where
    Self: Sized,
{
    fn handle<M: MessageView>(
        &mut self,
        resources: &mut SimResources<R>,
        message: &M,
    ) -> Option<StateInstruction<S>>;
}

pub type OpenSimHandlerBuilder<T, R, S> =
    OpenMessageHandlerBuilder<T, SimResources<R>, StateInstruction<S>>;

pub type ClosedSimHandlerBuilder<T, R, S> =
    ClosedMessageHandlerBuilder<T, SimResources<R>, StateInstruction<S>>;

pub type InitSimHandlerBuilder<T, R, S> =
    InitMessageHandlerBuilder<T, SimResources<R>, StateInstruction<S>>;

pub type SimHandler<T, R, S> = MessageHandler<T, SimResources<R>, StateInstruction<S>>;

pub struct SimStackEntry<T>(usize, Option<StateInstruction<T>>);

struct SimHState<R, S: SimState<R>> {
    states: Vec<S>,
    head_message: Option<InlineMessageView<SimStateEvent>>,
    tail_message: Option<InlineMessageView<SimStateEvent>>,
    stop_message: Option<InlineMessageView<SimStateEvent>>,
    stack: Vec<SimStackEntry<S>>,
    _pd: PhantomData<R>,
}

impl<R: 'static, S: SimState<R>> SimHState<R, S> {
    pub fn initial<A: AsRef<RuntimeContext>>(initial: S, res: A) -> Self {
        let initial_stack_entry = SimStackEntry(0, Some(StateInstruction::push(initial)));
        let head_message = InlineMessageView::new(SimStateEvent::Head, &res);
        let tail_message = InlineMessageView::new(SimStateEvent::Tail, &res);
        let stop_message = InlineMessageView::new(SimStateEvent::Stop, &res);

        Self {
            states: vec![],
            head_message,
            tail_message,
            stop_message,
            stack: vec![initial_stack_entry],
            _pd: Default::default(),
        }
    }

    fn inject_state_event(
        state: &mut S,
        res: &mut SimResources<R>,
        message: &Option<InlineMessageView<SimStateEvent>>,
    ) -> Option<StateInstruction<S>> {
        message.as_ref().and_then(|m| state.handle(res, m))
    }

    fn handle<M: MessageView>(&mut self, res: &mut SimResources<R>, message: &M) {
        // TODO: pop on shutdown event?
        // Optimization: filter messages that are not handed by any state

        let mut current: usize = 0;

        // Optimal path with no stat changes
        for state in &mut self.states {
            match state.handle(res, message) {
                None => {
                    current += 1;
                }
                Some(StateInstruction::Stay) => {
                    current += 1;
                }
                Some(instruction) => {
                    let stack_entry = SimStackEntry(current, Some(instruction));
                    self.stack.push(stack_entry);
                    current += 1;
                    break;
                }
            }
        }

        // println!("states.len: {}", self.states.len());
        // TODO: print stack state? State name?
        loop {
            match self.stack.pop() {
                None => {
                    let state = some_or_return!(self.states.get_mut(current as usize));
                    let instruction = state.handle(res, message);
                    self.stack
                        .push(SimStackEntry(current as usize, instruction));
                    current += 1;
                }
                Some(SimStackEntry(_at, None)) => {}
                Some(SimStackEntry(_at, Some(StateInstruction::Stay))) => {}
                Some(SimStackEntry(at, Some(StateInstruction::Switch(new)))) => {
                    log::info!("switch state at {}", at);

                    let is_head = at + 1 == self.states.len();
                    let state = self.states.get_mut(at).unwrap();
                    let instruction = Self::inject_state_event(state, res, &self.stop_message);
                    assert!(
                        instruction.map(|i| i.is_stay()).unwrap_or(true),
                        "non stay instructions are not supported when state handles SimStateEvent::Stop"
                    );
                    *state = new;

                    if is_head {
                        let instruction = Self::inject_state_event(state, res, &self.head_message);
                        self.stack.push(SimStackEntry(at, instruction));
                    } else {
                        let instruction = Self::inject_state_event(state, res, &self.tail_message);
                        self.stack.push(SimStackEntry(at, instruction));
                    }

                    if at < current {
                        current = at;
                    }
                }
                Some(SimStackEntry(at, Some(StateInstruction::Push(new)))) => {
                    log::info!("push {} state(s) at {}", new.len(), at);

                    let next_idx = at + 1;
                    let is_tail = next_idx < self.states.len();
                    let new_len = new.len();

                    if is_tail {
                        for mut state in self.states.drain(next_idx..).rev() {
                            let instruction =
                                Self::inject_state_event(&mut state, res, &self.stop_message);
                            assert!(
                                instruction.map(|i| i.is_stay()).unwrap_or(true),
                                "non stay instructions are not supported when state handles SimStateEvent::Stop"
                            );
                        }
                    }

                    if new_len > 0 {
                        self.states.extend(new);

                        for (idx, new_tail) in self
                            .states
                            .iter_mut()
                            .enumerate()
                            .skip(next_idx - !is_tail as usize)
                            .take(new_len + !is_tail as usize - 1)
                        {
                            let instruction =
                                Self::inject_state_event(new_tail, res, &self.tail_message);
                            self.stack.push(SimStackEntry(idx, instruction));
                        }
                    }

                    if (new_len == 0 && is_tail) || new_len > 0 {
                        let last_idx = self.states.len().saturating_sub(1);
                        if let Some(last) = self.states.last_mut() {
                            let instruction =
                                Self::inject_state_event(last, res, &self.head_message);
                            self.stack.push(SimStackEntry(last_idx, instruction));
                        }
                    }
                }
                Some(SimStackEntry(at, Some(StateInstruction::Pop))) => {
                    log::info!("pop state at {}", at);

                    for mut state in self.states.drain(at..).rev() {
                        let instruction =
                            Self::inject_state_event(&mut state, res, &self.stop_message);
                        assert!(
                            instruction.map(|i| i.is_stay()).unwrap_or(true),
                            "non stay instructions are not supported when state handles SimStateEvent::Stop"
                        );
                    }

                    let last_idx = self.states.len().saturating_sub(1);
                    if let Some(last) = self.states.last_mut() {
                        let instruction = Self::inject_state_event(last, res, &self.head_message);
                        self.stack.push(SimStackEntry(last_idx, instruction));
                    } else {
                        res.context.shutdown_switch().request_shutdown();
                    }
                }
                Some(SimStackEntry(at, Some(StateInstruction::PopPush(new)))) => {
                    log::info!("pop push {} state(s) at {}", new.len(), at);

                    let new_len = new.len();

                    for mut state in self.states.drain(at..).rev() {
                        let instruction =
                            Self::inject_state_event(&mut state, res, &self.stop_message);
                        assert!(
                            instruction.map(|i| i.is_stay()).unwrap_or(true),
                            "non stay instructions are not supported when state handles SimStateEvent::Stop"
                        );
                    }

                    if new_len > 0 {
                        self.states.extend(new);

                        for (idx, new_tail) in self
                            .states
                            .iter_mut()
                            .enumerate()
                            .skip(at)
                            .take(new_len - 1)
                        {
                            let instruction =
                                Self::inject_state_event(new_tail, res, &self.tail_message);
                            self.stack.push(SimStackEntry(idx, instruction));
                        }
                    }

                    let last_idx = self.states.len().saturating_sub(1);
                    if let Some(last) = self.states.last_mut() {
                        let instruction = Self::inject_state_event(last, res, &self.head_message);
                        self.stack.push(SimStackEntry(last_idx, instruction));
                    } else {
                        res.context.shutdown_switch().request_shutdown();
                    }

                    if at < current {
                        current = at;
                    }
                }
                Some(SimStackEntry(at, Some(StateInstruction::Extract))) => {
                    log::info!("extract state at {}", at);

                    let mut state = self.states.remove(at);
                    let instruction = Self::inject_state_event(&mut state, res, &self.stop_message);
                    assert!(
                        instruction.map(|i| i.is_stay()).unwrap_or(true),
                        "non stay instructions are not supported when state handles SimStateEvent::Stop"
                    );

                    if at == self.states.len() {
                        let last_idx = self.states.len().saturating_sub(1);
                        if let Some(last) = self.states.last_mut() {
                            let instruction =
                                Self::inject_state_event(last, res, &self.head_message);
                            self.stack.push(SimStackEntry(last_idx, instruction));
                        }
                    } else if self.states.is_empty() {
                        res.context.shutdown_switch().request_shutdown();
                    }

                    if at < current {
                        current = at;
                    }
                }
            }
        }
    }
}

pub struct SimServer<'a, R, S> {
    group: MessageGroupBuilder<'a>,
    resource_init: Box<dyn FnOnce() -> R + Send + 'static>,
    _pd: PhantomData<S>,
}

impl<'a, R: 'static, S: SimState<R>> SimServer<'a, R, S> {
    // TODO: improve UX
    pub fn builder<F>(group: MessageGroupBuilder<'a>, resource_init: F) -> Self
    where
        F: FnOnce() -> R + Send + 'static,
    {
        SimServer {
            group,
            resource_init: Box::new(resource_init),
            _pd: Default::default(),
        }
    }

    pub fn init_fn<F>(mut self, state_init: F) -> MessageGroup
    where
        F: FnOnce(&SimResources<R>) -> S + Send + 'static,
    {
        let setup_builder = self.group.register(|b| {
            b.on(on_assets_created_event)
                .on(on_render_created_event)
                .on(on_frame_requested_event)
                .init_default()
        });

        let simulated_builder = self
            .group
            .register(|b| b.on(on_frame_requested_event::<()>).init_default());

        let resource_init = self.resource_init;

        self.group.init(move |mut recv, mut context| {
            let mut setup = setup_builder.finish(&context).unwrap();
            let mut simulated = simulated_builder.finish(&context).unwrap();

            let setup_result = recv.recv_while(|message| {
                setup.handle(&mut context, message);
                setup.state.assets.is_none() || setup.state.render.is_none()
            });
            if let Err(e) = setup_result {
                log::info!(
                    "don't start sim server as runtime has already been shutdown: {}",
                    e
                );
                return;
            }

            log::info!("starting sim server");

            let mut res = SimResources {
                context,
                assets: setup.state.assets.unwrap(),
                render: setup.state.render.unwrap(),
                resource: (resource_init)(),
            };

            let initial_state = state_init(&res);
            let mut h_state = SimHState::initial(initial_state, &res);

            recv.stream(|message| {
                h_state.handle(&mut res, message);
                simulated.handle(&mut res.context, message);
            })
        })
    }
}

#[derive(Default)]
struct SimServerSetup {
    assets: Option<Assets>,
    render: Option<RenderClient>,
}

fn on_assets_created_event(
    state: &mut SimServerSetup,
    context: &mut RuntimeContext,
    event: &AssetsCreatedEvent,
) {
    state.assets = Some(event.assets(context.sender().to_owned()));
}

fn on_render_created_event(
    state: &mut SimServerSetup,
    context: &mut RuntimeContext,
    event: &RenderCreatedEvent,
) {
    state.render = Some(event.render_client(context.sender().to_owned()));
}

fn on_frame_requested_event<T>(
    _state: &mut T,
    context: &mut RuntimeContext,
    event: &FrameRequestedEvent,
) {
    context.sender().send(SimulatedEvent { frame: event.frame });
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SimStateEvent {
    Stop,
    Head,
    Tail,
}

#[derive(Debug, Clone, Copy)]
pub struct SimulatedEvent {
    pub frame: u64,
}

// TODO: test SimHState
