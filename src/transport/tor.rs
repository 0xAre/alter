//! Tor transport (M2/M5) — onion service hosting + connect via arti-client.
//!
//! Setiap peer menjalankan satu onion service persisten (alamat stabil, disimpan
//! di state dir arti). Alamat .onion dibagikan via invite code.
//!
//! M5 (SEC-13): restricted discovery — onion service dikonfigurasi dengan daftar
//! static keys sehingga hanya kontak yang sudah dikenal (yang punya x25519 keypair
//! kita) yang bisa menemukan/decrypt descriptor.
//!
//! Client-side: sebelum dial, inject our secret auth key ke keystore arti agar
//! arti bisa decrypt descriptor peer yang restricted.

use std::sync::Arc;
use std::time::Duration;

use arti_client::config::CfgPath;
use arti_client::{DataStream, TorClient, TorClientConfig};
use futures::StreamExt as _;
use safelog::DisplayRedacted as _;
use tokio::sync::{mpsc, Mutex};
use tor_cell::relaycell::msg::Connected;
use tor_hscrypto::pk::HsClientDescEncSecretKey;
use tor_hsservice::config::OnionServiceConfigBuilder;
use tor_hsservice::{handle_rend_requests, RunningOnionService};
use tor_llcrypto::pk::curve25519;
use tor_rtcompat::PreferredRuntime;

use crate::error::Error;

/// Virtual port yang dipakai initiator saat connect ke onion peer.
pub const TOR_VIRTUAL_PORT: u16 = 9999;

/// Konteks Tor aktif: client ter-bootstrap + onion service kita yang berjalan.
pub struct TorContext {
    client: Arc<TorClient<PreferredRuntime>>,
    _service: Arc<RunningOnionService>,
    pub onion_address: String,
    incoming: Mutex<mpsc::UnboundedReceiver<DataStream>>,
    /// Nickname service, dibutuhkan untuk restart.
    nickname: String,
}

impl TorContext {
    /// Bootstrap Tor, launch onion service, mulai accept loop.
    ///
    /// `authorized_keys`: daftar x25519 pubkeys kontak yang diizinkan mendekripsi
    /// descriptor kita (restricted discovery). Kosong = descriptor publik.
    pub async fn launch(
        cache_dir: &str,
        state_dir: &str,
        nickname: &str,
        authorized_keys: &[[u8; 32]],
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

        let ctx = Self::launch_service(client, nickname, authorized_keys).await?;
        Ok(ctx)
    }

    /// Restart onion service dengan daftar authorized keys baru.
    ///
    /// Dipakai saat kontak baru ditambah — service perlu restart (~5 dtk) agar
    /// kunci baru ditambahkan ke descriptor. TorClient (Tor connection pool) tetap
    /// sama; hanya onion service yang di-restart.
    pub async fn restart_with_authorized_keys(
        &self,
        new_keys: &[[u8; 32]],
    ) -> Result<Arc<Self>, Error> {
        Self::launch_service(Arc::clone(&self.client), &self.nickname, new_keys).await
    }

    /// Inject client auth secret key ke keystore arti untuk satu onion address.
    ///
    /// Harus dipanggil sebelum melakukan dial ke onion peer yang punya restricted
    /// discovery aktif. Injeksi idempoten — aman dipanggil berulang kali.
    pub async fn register_client_auth_key(
        &self,
        onion_host: &str,
        our_secret_seed: [u8; 32],
    ) -> Result<(), Error> {
        use std::str::FromStr as _;
        let hsid = arti_client::HsId::from_str(onion_host)
            .map_err(|e| Error::Tor(format!("alamat onion tidak valid: {e}")))?;
        let secret: HsClientDescEncSecretKey =
            curve25519::StaticSecret::from(our_secret_seed).into();
        self.client
            .insert_service_discovery_key(
                arti_client::KeystoreSelector::Primary,
                hsid,
                secret,
            )
            .map(|_| ())
            .map_err(|e| Error::Tor(e.to_string()))
    }

    /// Connect ke onion peer via Tor (sebagai initiator).
    pub async fn connect(&self, onion_host: &str, port: u16) -> Result<DataStream, Error> {
        self.client
            .connect((onion_host.to_string(), port))
            .await
            .map_err(|e| Error::Tor(e.to_string()))
    }

    /// Tunggu stream masuk berikutnya dengan timeout.
    pub async fn accept_timeout(&self, timeout: Duration) -> Option<DataStream> {
        let mut rx = self.incoming.lock().await;
        tokio::time::timeout(timeout, rx.recv()).await.ok().flatten()
    }

    // ─── Internal ──────────────────────────────────────────────────────────────

    async fn launch_service(
        client: Arc<TorClient<PreferredRuntime>>,
        nickname: &str,
        authorized_keys: &[[u8; 32]],
    ) -> Result<Arc<Self>, Error> {
        let parsed_nick = nickname
            .parse()
            .map_err(|e| Error::Tor(format!("nickname tidak valid: {e}")))?;

        let svc_cfg = build_service_config(parsed_nick, authorized_keys)?;

        let (service, rend_stream) = client
            .launch_onion_service(svc_cfg)
            .map_err(|e| Error::Tor(e.to_string()))?
            .ok_or_else(|| Error::Tor("onion service disabled di config".into()))?;

        let onion_address = service
            .onion_address()
            .ok_or_else(|| Error::Tor("alamat onion belum tersedia".into()))?
            .display_unredacted()
            .to_string();

        let (in_tx, in_rx) = mpsc::unbounded_channel::<DataStream>();
        tokio::spawn(async move {
            let mut streams = Box::pin(handle_rend_requests(rend_stream));
            while let Some(req) = streams.next().await {
                match req.accept(Connected::new_empty()).await {
                    Ok(ds) => {
                        if in_tx.send(ds).is_err() {
                            break;
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
            nickname: nickname.to_string(),
        }))
    }
}

fn build_service_config(
    nickname: tor_hsservice::HsNickname,
    authorized_keys: &[[u8; 32]],
) -> Result<tor_hsservice::config::OnionServiceConfig, Error> {
    let mut svc_builder = OnionServiceConfigBuilder::default();
    svc_builder.nickname(nickname);

    if !authorized_keys.is_empty() {
        use tor_hscrypto::pk::HsClientDescEncKey;
        use tor_hsservice::config::restricted_discovery::HsClientNickname;

        let rd = svc_builder.restricted_discovery();
        rd.enabled(true);
        for (i, key_bytes) in authorized_keys.iter().enumerate() {
            let nick: HsClientNickname = format!("client-{i}")
                .parse()
                .map_err(|e| Error::Tor(format!("nickname restricted_discovery tidak valid: {e}")))?;
            let enc_key: HsClientDescEncKey = curve25519::PublicKey::from(*key_bytes).into();
            rd.static_keys().access().push((nick, enc_key));
        }
    }

    svc_builder.build().map_err(|e| Error::Tor(e.to_string()))
}
