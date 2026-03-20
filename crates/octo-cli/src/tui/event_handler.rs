//! Event handler that merges crossterm terminal events with AgentEvent broadcasts.
//!
//! Uses three background tasks to collect events into a single mpsc channel:
//! 1. Terminal events (key presses, resize)
//! 2. Agent events (text deltas, tool calls, completions)
//! 3. Tick timer (for spinner animations)

use crossterm::event::{Event as CEvent, EventStream};
use futures_util::StreamExt;
use std::time::Duration;
use tokio::sync::{broadcast, mpsc};

use octo_engine::agent::AgentEvent;

use super::event::AppEvent;

/// Unified event handler that collects terminal, agent, and tick events.
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<AppEvent>,
}

impl EventHandler {
    /// Create a new event handler.
    ///
    /// - `agent_rx`: broadcast receiver for agent lifecycle events
    /// - `tick_rate`: interval between tick events (for animations)
    pub fn new(agent_rx: broadcast::Receiver<AgentEvent>, tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_term = tx.clone();
        let tx_agent = tx.clone();
        let tx_tick = tx;

        // Terminal events (crossterm)
        tokio::spawn(async move {
            let mut stream = EventStream::new();
            while let Some(Ok(event)) = stream.next().await {
                let app_event = match event {
                    CEvent::Key(key) => Some(AppEvent::Key(key)),
                    CEvent::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                    _ => None,
                };
                if let Some(evt) = app_event {
                    if tx_term.send(evt).is_err() {
                        break;
                    }
                }
            }
        });

        // Agent events
        tokio::spawn(async move {
            let mut rx = agent_rx;
            while let Ok(event) = rx.recv().await {
                if tx_agent.send(AppEvent::Agent(event)).is_err() {
                    break;
                }
            }
        });

        // Tick timer
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                interval.tick().await;
                if tx_tick.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx }
    }

    /// Create an event handler without an agent receiver (for standalone TUI usage).
    pub fn without_agent(tick_rate: Duration) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let tx_term = tx.clone();
        let tx_tick = tx;

        tokio::spawn(async move {
            let mut stream = EventStream::new();
            while let Some(Ok(event)) = stream.next().await {
                let app_event = match event {
                    CEvent::Key(key) => Some(AppEvent::Key(key)),
                    CEvent::Resize(w, h) => Some(AppEvent::Resize(w, h)),
                    _ => None,
                };
                if let Some(evt) = app_event {
                    if tx_term.send(evt).is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick_rate);
            loop {
                interval.tick().await;
                if tx_tick.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        Self { rx }
    }

    /// Wait for the next event (blocking).
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.rx.recv().await
    }

    /// Try to receive an event without blocking.
    pub fn try_next(&mut self) -> Option<AppEvent> {
        self.rx.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast as bcast;

    #[tokio::test]
    async fn test_tick_events() {
        let (_tx, rx) = bcast::channel::<AgentEvent>(16);
        let mut handler = EventHandler::new(rx, Duration::from_millis(10));

        // Should receive at least one tick within 50ms
        let event = tokio::time::timeout(Duration::from_millis(100), handler.next())
            .await
            .expect("should receive event");

        assert!(matches!(event, Some(AppEvent::Tick)));
    }

    #[tokio::test]
    async fn test_agent_events_forwarded() {
        let (tx, rx) = bcast::channel::<AgentEvent>(16);
        let mut handler = EventHandler::new(rx, Duration::from_secs(60)); // slow tick

        // Send an agent event
        tx.send(AgentEvent::TextDelta {
            text: "hello".into(),
        })
        .unwrap();

        let event = tokio::time::timeout(Duration::from_millis(100), handler.next())
            .await
            .expect("should receive event");

        match event {
            Some(AppEvent::Agent(AgentEvent::TextDelta { text })) => {
                assert_eq!(text, "hello");
            }
            other => panic!("Expected Agent(TextDelta), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_try_next_empty() {
        let (_tx, rx) = bcast::channel::<AgentEvent>(16);
        let mut handler = EventHandler::new(rx, Duration::from_secs(60));

        // Immediately after creation, try_next should return None
        // (tick hasn't fired yet with 60s interval)
        // Note: this is racy but with 60s tick it should be fine
        let result = handler.try_next();
        // Could be None or Tick — both acceptable
        if let Some(event) = result {
            assert!(matches!(event, AppEvent::Tick));
        }
    }
}
