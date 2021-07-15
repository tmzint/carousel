use crate::asset::storage::Assets;
use crate::asset::AssetsCreatedEvent;
use crate::platform::message::FrameRequestedEvent;
use crate::render::client::RenderClient;
use crate::render::message::RenderCreatedEvent;
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

struct SimHState<R, S: SimState<R>> {
    states: Vec<S>,
    head_message: Option<InlineMessageView<SimStateEvent>>,
    tail_message: Option<InlineMessageView<SimStateEvent>>,
    stop_message: Option<InlineMessageView<SimStateEvent>>,
    _pd: PhantomData<R>,
}

impl<R: 'static, S: SimState<R>> SimHState<R, S> {
    pub fn initial<A: AsRef<RuntimeContext>>(initial: S, res: A) -> Self {
        let head_message = InlineMessageView::new(SimStateEvent::Head, &res);
        let tail_message = InlineMessageView::new(SimStateEvent::Tail, &res);
        let stop_message = InlineMessageView::new(SimStateEvent::Stop, &res);

        Self {
            states: vec![initial],
            head_message,
            tail_message,
            stop_message,
            _pd: Default::default(),
        }
    }

    fn inject_state_event(
        state: &mut S,
        res: &mut SimResources<R>,
        message: &Option<InlineMessageView<SimStateEvent>>,
    ) {
        if let Some(message) = message {
            let instruction = state.handle(res, message);
            // TODO: support instructions?
            assert!(
                instruction.map(|i| i.is_stay()).unwrap_or(true),
                "non stay instructions are not supported when state handles SimStateEvent"
            );
        };
    }

    fn handle<M: MessageView>(&mut self, res: &mut SimResources<R>, message: &M) {
        // TODO: implement instruction handling via a stack machine?
        let mut current = 0;
        while current < self.states.len() {
            let shadow = current + 1 < self.states.len();
            let instruction = self.states.get_mut(current).unwrap().handle(res, message);
            match instruction {
                None => {
                    current += 1;
                }
                Some(StateInstruction::Stay) => {
                    current += 1;
                }
                Some(StateInstruction::Switch(new)) => {
                    let state = self.states.get_mut(current).unwrap();
                    Self::inject_state_event(state, res, &self.stop_message);
                    *state = new;
                }
                Some(StateInstruction::Push(mut new)) => {
                    if shadow {
                        for mut state in self.states.drain(current + 1..).rev() {
                            Self::inject_state_event(&mut state, res, &self.stop_message);
                        }
                    } else if new.len() > 0 {
                        let state = self.states.get_mut(current).unwrap();
                        Self::inject_state_event(state, res, &self.tail_message);
                    }

                    let new_tails = new.len().saturating_sub(1);
                    for new_tail in new.iter_mut().take(new_tails) {
                        Self::inject_state_event(new_tail, res, &self.tail_message);
                    }
                    self.states.extend(new);
                    current += 1;
                }
                Some(StateInstruction::Pop) => {
                    for mut state in self.states.drain(current..).rev() {
                        Self::inject_state_event(&mut state, res, &self.stop_message);
                    }

                    if let Some(state) = self.states.get_mut(current.saturating_sub(1)) {
                        Self::inject_state_event(state, res, &self.head_message);
                    }

                    if self.states.is_empty() {
                        res.context.shutdown_switch().request_shutdown();
                    }
                }
                Some(StateInstruction::PopPush(mut new)) => {
                    for mut state in self.states.drain(current..).rev() {
                        Self::inject_state_event(&mut state, res, &self.stop_message);
                    }

                    if new.is_empty() {
                        if let Some(state) = self.states.get_mut(current.saturating_sub(1)) {
                            Self::inject_state_event(state, res, &self.head_message);
                        }
                    }

                    let new_tails = new.len().saturating_sub(1);
                    for new_tail in new.iter_mut().take(new_tails) {
                        Self::inject_state_event(new_tail, res, &self.tail_message);
                    }

                    self.states.extend(new);

                    if self.states.is_empty() {
                        res.context.shutdown_switch().request_shutdown();
                    }
                }
                Some(StateInstruction::Extract) => {
                    let mut state = self.states.remove(current);
                    Self::inject_state_event(&mut state, res, &self.stop_message);
                    if current == self.states.len() {
                        if let Some(state) = self.states.get_mut(current.saturating_sub(1)) {
                            Self::inject_state_event(state, res, &self.head_message);
                        }
                    }

                    if self.states.is_empty() {
                        res.context.shutdown_switch().request_shutdown();
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
