#![forbid(unsafe_code)]

//! Iced/layer-shell frontend adapter. No framework types escape this crate.

use std::{
    hash::{Hash, Hasher},
    sync::{Arc, Mutex},
};

use iced::{
    Element, Event, Length, Subscription, Task, Theme, event, keyboard, stream,
    widget::{column, container, text, text_input},
    window,
};
use iced_layershell::{
    application,
    reexport::Anchor,
    settings::{LayerShellSettings, Settings, StartMode},
    to_layer_message,
};
use loncher_ui_contract::{
    UiBackend, UiCommand, UiError, UiEvent, UiReceipt, UiSnapshot, UiVisibility,
};

use iced::futures::{SinkExt, StreamExt, channel::mpsc};

#[derive(Clone)]
pub struct IcedUiBackend {
    snapshot_tx: mpsc::Sender<UiSnapshot>,
}

pub struct FrontendChannels {
    pub backend: IcedUiBackend,
    snapshot_rx: mpsc::Receiver<UiSnapshot>,
    event_tx: mpsc::Sender<UiEvent>,
    pub event_rx: mpsc::Receiver<UiEvent>,
}

pub fn channels() -> FrontendChannels {
    let (snapshot_tx, snapshot_rx) = mpsc::channel(32);
    let (event_tx, event_rx) = mpsc::channel(32);
    FrontendChannels { backend: IcedUiBackend { snapshot_tx }, snapshot_rx, event_tx, event_rx }
}

impl UiBackend for IcedUiBackend {
    fn dispatch(&mut self, command: UiCommand) -> Result<UiReceipt, UiError> {
        match command {
            UiCommand::ApplySnapshot(snapshot) => {
                self.snapshot_tx
                    .try_send(snapshot)
                    .map_err(|_| UiError::Rejected("GUI event queue is closed"))?;
                Ok(UiReceipt::Accepted)
            }
            UiCommand::Shutdown => {
                self.snapshot_tx
                    .try_send(UiSnapshot { generation: u64::MAX, ..UiSnapshot::default() })
                    .map_err(|_| UiError::Rejected("GUI event queue is closed"))?;
                Ok(UiReceipt::Accepted)
            }
        }
    }
}

#[to_layer_message]
#[derive(Debug, Clone)]
enum Message {
    Snapshot(UiSnapshot),
    QueryChanged(String),
    Event(Event),
}

pub fn run(channels: FrontendChannels) -> Result<(), iced_layershell::Error> {
    let FrontendChannels { snapshot_rx, event_tx, .. } = channels;
    let snapshot_source = Arc::new(SnapshotSource { receiver: Mutex::new(Some(snapshot_rx)) });
    let initial = UiSnapshot::default();
    let program = application(
        move || FrontendState { snapshot: initial.clone(), event_tx: event_tx.clone() },
        || "loncher-launcher".to_owned(),
        update,
        view,
    )
    .subscription(move |_| frontend_subscription(snapshot_source.clone()))
    .settings(Settings {
        layer_settings: LayerShellSettings {
            anchor: Anchor::Top | Anchor::Left | Anchor::Right,
            size: Some((0, 420)),
            start_mode: StartMode::Active,
            ..Default::default()
        },
        ..Default::default()
    });
    program.run()
}

struct FrontendState {
    snapshot: UiSnapshot,
    event_tx: mpsc::Sender<UiEvent>,
}

fn update(state: &mut FrontendState, message: Message) -> Task<Message> {
    match message {
        Message::Snapshot(snapshot) if snapshot.generation == u64::MAX => {
            return window::latest().then(|id| match id {
                Some(id) => window::close(id),
                None => Task::none(),
            });
        }
        Message::Snapshot(snapshot) => state.snapshot = snapshot,
        Message::QueryChanged(query) => {
            state.snapshot.query = Some(query.clone());
            let _ = state.event_tx.try_send(UiEvent::QueryChanged(query));
        }
        Message::Event(Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })) => {
            match key {
                keyboard::Key::Named(keyboard::key::Named::Escape) => {
                    let _ = state.event_tx.try_send(UiEvent::DismissRequested);
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowDown) if modifiers.is_empty() => {
                    let _ = state.event_tx.try_send(UiEvent::MoveSelection { delta: 1 });
                }
                keyboard::Key::Named(keyboard::key::Named::ArrowUp) if modifiers.is_empty() => {
                    let _ = state.event_tx.try_send(UiEvent::MoveSelection { delta: -1 });
                }
                keyboard::Key::Named(keyboard::key::Named::Enter) => {
                    let _ = state.event_tx.try_send(UiEvent::SubmitRequested);
                }
                keyboard::Key::Named(keyboard::key::Named::Tab) => {
                    let _ = state.event_tx.try_send(UiEvent::CompleteSelection);
                }
                _ => {}
            }
        }
        Message::Event(_) => {}
        _ => {}
    }
    Task::none()
}

fn view(state: &FrontendState) -> Element<'_, Message, Theme, iced::Renderer> {
    if state.snapshot.visibility == UiVisibility::Hidden {
        return container(text("")).height(Length::Fixed(1.0)).into();
    }
    let query = state.snapshot.query.clone().unwrap_or_default();
    let input =
        text_input("Search applications…", &query).on_input(Message::QueryChanged).padding(12);
    let results = state.snapshot.results.iter().enumerate().fold(
        column![input].spacing(8),
        |column, (index, result)| {
            let marker = if index == state.snapshot.selected { "▸ " } else { "  " };
            let generic = result.generic_name.as_deref().unwrap_or("");
            column.push(text(format!("{marker}{}  {generic}", result.name)).size(18))
        },
    );
    container(results.padding(24)).width(Length::Fill).into()
}

struct SnapshotSource {
    receiver: Mutex<Option<mpsc::Receiver<UiSnapshot>>>,
}

impl Hash for SnapshotSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self as *const Self as usize).hash(state);
    }
}

fn frontend_subscription(source: Arc<SnapshotSource>) -> Subscription<Message> {
    let events = event::listen().map(Message::Event);
    Subscription::batch([Subscription::run_with(source, snapshot_stream), events])
}

fn snapshot_stream(
    source: &Arc<SnapshotSource>,
) -> iced::futures::stream::BoxStream<'static, Message> {
    let receiver = source.receiver.lock().ok().and_then(|mut receiver| receiver.take());
    stream::channel(32, async move |mut output| {
        let Some(mut receiver) = receiver else { return };
        while let Some(snapshot) = receiver.next().await {
            if output.send(Message::Snapshot(snapshot)).await.is_err() {
                break;
            }
        }
    })
    .boxed()
}
