//! SSE hub — broadcast progres job per transcript (docs/02 §6).

use std::sync::Arc;

use dashmap::DashMap;
use futures::stream::Stream;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SsePayload {
    Progress {
        status: String,
        progress: i16,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Completed {
        transcript_id: Uuid,
        duration_seconds: Option<f64>,
        language: Option<String>,
    },
    Failed {
        message: String,
    },
    Cancelled,
}

#[derive(Clone, Default)]
pub struct SseHub {
    /// Arc: AppState di-clone lintas worker/router — subscriber table HARUS shared.
    subscribers: Arc<DashMap<Uuid, Vec<mpsc::Sender<SsePayload>>>>,
}

const CHANNEL_CAPACITY: usize = 32;

impl SseHub {
    pub fn new() -> Self {
        Self::default()
    }

    /// Daftarkan subscriber → stream event untuk klien SSE.
    /// Boxed + Send: stream hidup lebih lama dari handler (body SSE), tidak meminjam hub.
    pub fn subscribe(
        &self,
        transcript_id: Uuid,
    ) -> std::pin::Pin<Box<dyn Stream<Item = SsePayload> + Send>> {
        let (tx, rx) = mpsc::channel::<SsePayload>(CHANNEL_CAPACITY);
        self.subscribers.entry(transcript_id).or_default().push(tx);
        Box::pin(ReceiverStream::new(rx))
    }

    /// Kirim payload ke semua subscriber; channel mati dibuang (lazy cleanup).
    pub fn publish(&self, transcript_id: Uuid, payload: SsePayload) {
        if let Some(mut senders) = self.subscribers.get_mut(&transcript_id) {
            senders.retain(|tx| tx.try_send(payload.clone()).is_ok());
        }
    }

    /// Bersihkan subscriber tanpa penerima (dipanggil saat stream selesai).
    pub fn cleanup(&self, transcript_id: Uuid) {
        if let Some(mut senders) = self.subscribers.get_mut(&transcript_id) {
            senders.retain(|tx| !tx.is_closed());
            if senders.is_empty() {
                drop(senders);
                self.subscribers.remove(&transcript_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn publish_terima_oleh_subscriber() {
        let hub = SseHub::new();
        let id = Uuid::now_v7();
        let mut stream = hub.subscribe(id);

        hub.publish(
            id,
            SsePayload::Progress {
                status: "transcribing".into(),
                progress: 42,
                message: Some("chunk 1/2".into()),
            },
        );
        hub.publish(
            id,
            SsePayload::Completed {
                transcript_id: id,
                duration_seconds: Some(1.5),
                language: Some("id".into()),
            },
        );

        let first = stream.next().await.unwrap();
        assert!(matches!(first, SsePayload::Progress { progress: 42, .. }));
        let second = stream.next().await.unwrap();
        assert!(matches!(second, SsePayload::Completed { .. }));
    }

    #[tokio::test]
    async fn clone_berbagi_subscriber_table() {
        // Worker (clone) publish → subscriber di instance lain menerima.
        let hub_a = SseHub::new();
        let hub_b = hub_a.clone();
        let id = Uuid::now_v7();
        let mut stream = hub_b.subscribe(id);

        hub_a.publish(
            id,
            SsePayload::Progress {
                status: "transcribing".into(),
                progress: 1,
                message: None,
            },
        );
        assert!(stream.next().await.is_some());
    }

    #[tokio::test]
    async fn subscriber_mati_dibuang() {
        let hub = SseHub::new();
        let id = Uuid::now_v7();
        {
            let _stream = hub.subscribe(id);
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        hub.publish(id, SsePayload::Cancelled); // tidak boleh panic
        hub.cleanup(id);
        assert!(!hub.subscribers.contains_key(&id));
    }
}
