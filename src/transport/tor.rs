//! Tor transport (M2) — onion service hosting + connect via arti-client.
//!
//! Setiap peer menjalankan satu onion service persisten (alamat stabil, disimpan
//! di state dir arti). Alamat .onion dibagikan via invite code. Initiator
//! menghubungi onion responder lewat sirkuit Tor; semua tetap dibungkus Noise_IK
//! di layer atas (defense-in-depth: Tor menyembunyikan jaringan, Noise mengamankan
//! konten + autentikasi identitas).
//!
//! Catatan: bootstrap Tor lambat (~30–60 dtk) dan butuh internet. TorContext
//! dibuat sekali saat startup bila flag `--tor` aktif.

use std::sync::Arc;
use std::time::Duration;

use arti_client::config::CfgPath;
use arti_client::{DataStream, TorClient, TorClientConfig};
use futures::StreamExt as _;
use safelog::DisplayRedacted as _;
use tokio::sync::{mpsc, Mutex};
use tor_cell::relaycell::msg::Connected;
use tor_hsservice::config::OnionServiceConfigBuilder;
use tor_hsservice::{handle_rend_requests, RunningOnionService};
use tor_rtcompat::PreferredRuntime;

use crate::error::Error;

/// Virtual port yang dipakai initiator saat connect ke onion peer.
/// Onion service tidak bind port OS nyata — ini hanya angka di protokol.
pub const TOR_VIRTUAL_PORT: u16 = 9999;

/// Konteks Tor aktif: client ter-bootstrap + onion service kita yang berjalan.
pub struct TorContext {
    client: Arc<TorClient<PreferredRuntime>>,
    /// Dipegang agar onion service tetap hidup selama context ada.
    _service: Arc<RunningOnionService>,
    /// Alamat .onion kita (untuk dibagikan via invite code).
    pub onion_address: String,
    /// Stream masuk dari onion service kita (sebagai responder).
    incoming: Mutex<mpsc::UnboundedReceiver<DataStream>>,
}

impl TorContext {
    /// Bootstrap Tor, launch onion service dengan `nickname`, dan mulai accept loop.
    /// `cache_dir` & `state_dir` adalah path absolut tempat arti menyimpan cache
    /// dan key (key onion persisten → alamat stabil).
    pub async fn launch(
        cache_dir: &str,
        state_dir: &str,
        nickname: &str,
    ) -> Result<Arc<Self>, Error> {
        let mut builder = TorClientConfig::builder();
        builder
            .storage()
            .cache_dir(CfgPath::new(cache_dir.to_string()))
            .state_dir(CfgPath::new(state_dir.to_string()));
        let config = builder.build().map_err(|e| Error::Tor(e.to_string()))?;

        let client = TorClient::create_bootstrapped(config)
            .await
            .map_err(|e| Error::Tor(e.to_string()))?;

        let svc_cfg = OnionServiceConfigBuilder::default()
            .nickname(
                nickname
                    .parse()
                    .map_err(|e| Error::Tor(format!("nickname tidak valid: {e}")))?,
            )
            .build()
            .map_err(|e| Error::Tor(e.to_string()))?;

        let (service, rend_stream) = client
            .launch_onion_service(svc_cfg)
            .map_err(|e| Error::Tor(e.to_string()))?
            .ok_or_else(|| Error::Tor("onion service disabled di config".into()))?;

        let onion_address = service
            .onion_address()
            .ok_or_else(|| Error::Tor("alamat onion belum tersedia".into()))?
            .display_unredacted()
            .to_string();

        // Accept loop: terima semua rend request → StreamRequest → DataStream.
        let (in_tx, in_rx) = mpsc::unbounded_channel::<DataStream>();
        tokio::spawn(async move {
            let mut streams = Box::pin(handle_rend_requests(rend_stream));
            while let Some(req) = streams.next().await {
                match req.accept(Connected::new_empty()).await {
                    Ok(ds) => {
                        if in_tx.send(ds).is_err() {
                            break; // konsumer berhenti
                        }
                    }
                    Err(_) => continue,
                }
            }
        });

        Ok(Arc::new(Self {
            client,
            _service: service,
            onion_address,
            incoming: Mutex::new(in_rx),
        }))
    }

    /// Connect ke onion peer via Tor (sebagai initiator).
    pub async fn connect(&self, onion_host: &str, port: u16) -> Result<DataStream, Error> {
        self.client
            .connect((onion_host.to_string(), port))
            .await
            .map_err(|e| Error::Tor(e.to_string()))
    }

    /// Tunggu stream masuk berikutnya dengan timeout.
    /// Mengembalikan `None` bila waktu habis atau channel tertutup.
    pub async fn accept_timeout(&self, timeout: Duration) -> Option<DataStream> {
        let mut rx = self.incoming.lock().await;
        tokio::time::timeout(timeout, rx.recv()).await.ok().flatten()
    }
}
