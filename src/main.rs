mod contacts;
mod crypto;
mod error;
mod identity;
mod platform;
mod session;
mod transport;
mod tui;

use std::io::{self, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc;

use zeroize::Zeroizing;

use crate::contacts::Contact;
use crate::error::Error;
use crate::identity::keypair::KeyBundle;
use crate::identity::vault;
use crate::transport::tor::TorContext;
use crate::tui::{ConnectKind, SelfKeys};

/// Argumen command line (parser manual — tanpa clap demi binary kecil).
struct Args {
    /// Subcommand `id`: cetak invite code lalu keluar.
    print_id: bool,
    vault_path: PathBuf,
    connect: ConnectKind,
    add_invite: Option<String>,
    add_name: Option<String>,
    /// Online (Tor) default aktif; `--offline` mematikannya (LAN murni).
    offline: bool,
    /// SEC-09: nama proses generik — visible di `ps aux` / Task Manager.
    process_name: Option<String>,
}

/// Home directory cross-platform (USERPROFILE di Windows, HOME di Unix).
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Lokasi data default: `~/.alter`. Vault & state Tor disimpan di sini supaya
/// `alter` dari folder mana pun membuka identitas yang sama (zero-setup).
fn default_data_dir() -> PathBuf {
    home_dir().join(".alter")
}

fn default_vault_path() -> PathBuf {
    default_data_dir().join("id.key")
}

impl Args {
    fn parse() -> Result<Self, String> {
        let mut print_id = false;
        let mut vault_path = default_vault_path();
        let mut listen: Option<u16> = None;
        let mut dial: Option<SocketAddr> = None;
        let mut add_invite = None;
        let mut add_name = None;
        let mut offline = false;
        let mut process_name: Option<String> = None;

        let mut it = std::env::args().skip(1);
        while let Some(arg) = it.next() {
            match arg.as_str() {
                "id" => print_id = true,
                "--vault" => {
                    vault_path = PathBuf::from(
                        it.next().ok_or("--vault butuh argumen path")?,
                    );
                }
                "--listen" => {
                    let p = it.next().ok_or("--listen butuh argumen port")?;
                    listen = Some(p.parse().map_err(|_| "port tidak valid")?);
                }
                "--dial" => {
                    let a = it.next().ok_or("--dial butuh argumen ip:port")?;
                    dial = Some(a.parse().map_err(|_| "alamat tidak valid")?);
                }
                "--add" => {
                    add_invite = Some(it.next().ok_or("--add butuh invite code")?);
                }
                "--name" => {
                    add_name = Some(it.next().ok_or("--name butuh nickname")?);
                }
                "--offline" => offline = true,
                "--process-name" => {
                    process_name = Some(it.next().ok_or("--process-name butuh argumen nama")?.to_string());
                }
                "--tor" => {} // diterima tapi no-op: online sudah default
                "-h" | "--help" => return Err(help_text()),
                other => return Err(format!("argumen tidak dikenal: {other}\n\n{}", help_text())),
            }
        }

        let connect = match (listen, dial) {
            (Some(_), Some(_)) => return Err("--listen dan --dial tidak bisa bersamaan".into()),
            (Some(p), None) => ConnectKind::Listen(p),
            (None, Some(a)) => ConnectKind::Dial(a),
            (None, None) => ConnectKind::Auto,
        };

        Ok(Args {
            print_id,
            vault_path,
            connect,
            add_invite,
            add_name,
            offline,
            process_name,
        })
    }
}

fn help_text() -> String {
    "\
ALTER — serverless encrypted P2P terminal chat

Penggunaan:
  alter [opsi]            Jalankan TUI
  alter id [opsi]         Cetak invite code lalu keluar

Opsi:
  --vault <path>          Lokasi file vault (default: ~/.alter/id.key)
  --add <invite>          Pra-muat satu kontak dari invite code
  --name <nickname>       Nickname untuk kontak --add
  --listen <port>         Paksa mode responder (listen) — untuk testing 1 mesin
  --dial <ip:port>        Paksa mode initiator (dial langsung) — untuk testing
  --offline               Matikan Tor (LAN murni; tak butuh internet)
  --process-name <name>   Set nama proses di ps/Task Manager (SEC-09)
  -h, --help              Tampilkan bantuan ini

Default: ONLINE (LAN + Tor). Tor bootstrap di latar belakang — TUI langsung jalan,
badge berubah jadi TOR+LAN saat siap. Passphrase id dibaca dari env ALTER_PASSPHRASE."
        .to_string()
}

/// Hitung path absolut (string) untuk cache & state dir Tor, diturunkan dari
/// nama file vault supaya dua instance (vault berbeda) tidak bentrok state.
fn tor_dirs(vault_path: &std::path::Path) -> (String, String) {
    let stem = vault_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "alter".to_string());
    let parent = vault_path.parent().filter(|p| !p.as_os_str().is_empty());
    let base = match parent {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let cache = base.join(format!("{stem}-tcache"));
    let state = base.join(format!("{stem}-tstate"));
    (
        cache.to_string_lossy().to_string(),
        state.to_string_lossy().to_string(),
    )
}

/// Baca passphrase tanpa echo: dari env (otomasi) atau crossterm raw mode (interaktif).
/// M4: menggantikan stdin read_line yang ter-echo. Return Zeroizing<String> agar
/// passphrase otomatis di-wipe dari memori saat tidak lagi dibutuhkan.
fn read_passphrase(prompt: &str) -> Result<Zeroizing<String>, Error> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use crossterm::terminal::{disable_raw_mode, enable_raw_mode};

    if let Ok(p) = std::env::var("ALTER_PASSPHRASE") {
        return Ok(Zeroizing::new(p));
    }

    print!("{prompt}");
    io::stdout().flush().ok();

    enable_raw_mode()?;
    let mut pass = Zeroizing::new(String::new());
    loop {
        match event::read() {
            Ok(Event::Key(k)) if k.kind == KeyEventKind::Press => match k.code {
                KeyCode::Enter => break,
                KeyCode::Char(c) => pass.push(c),
                KeyCode::Backspace => { pass.pop(); }
                KeyCode::Esc => {
                    disable_raw_mode().ok();
                    println!();
                    return Err(Error::KeyDerivation); // dibatalkan user
                }
                _ => {}
            },
            Err(e) => {
                disable_raw_mode().ok();
                return Err(Error::Io(e));
            }
            _ => {}
        }
    }
    disable_raw_mode().ok();
    println!(); // newline setelah input tersembunyi
    Ok(pass)
}

/// Muat vault dari disk, atau buat identitas baru bila belum ada.
fn load_or_create_vault(path: &std::path::Path) -> Result<KeyBundle, Error> {
    if path.exists() {
        let vault_bytes = vault::read_vault(path)?;
        let pass = read_passphrase("Passphrase: ")?;
        vault::unseal(&vault_bytes, pass.as_bytes())
    } else {
        println!("Vault tidak ditemukan di {}.", path.display());
        println!("Membuat identitas baru.");
        let pass = read_passphrase("Buat passphrase: ")?;
        if pass.is_empty() {
            return Err(Error::KeyDerivation);
        }
        let bundle = KeyBundle::generate();
        let vault_bytes = vault::seal(&bundle, pass.as_bytes())?;
        vault::write_vault(path, &vault_bytes)?;
        println!("Identitas dibuat dan disimpan.");
        Ok(bundle)
    }
}

fn build_self_keys(bundle: &KeyBundle, onion: Option<&str>) -> SelfKeys {
    let ed_pub = bundle.identity.public_key().to_bytes();
    let noise_pub = bundle.noise.public_bytes();
    let noise_sk = bundle.noise.secret_bytes();
    let cap_pub = contacts::derive_tor_client_auth_pub(bundle);
    let cap_secret = contacts::derive_tor_client_auth_secret_seed(bundle);
    let fingerprint = contacts::fingerprint(&ed_pub);
    let invite = contacts::encode_invite(&ed_pub, &noise_pub, &cap_pub, onion);
    let mut sk = SelfKeys {
        fingerprint,
        noise_sk,
        noise_pub,
        ed25519_pub: ed_pub,
        invite,
        tor_client_auth_pub: cap_pub,
        tor_client_auth_secret: cap_secret,
    };
    platform::try_mlock(sk.noise_sk.as_mut_ptr(), 32);
    platform::try_mlock(sk.tor_client_auth_secret.as_mut_ptr(), 32);
    sk
}

async fn real_main(args: Args) -> Result<(), Error> {
    // Pastikan folder data (mis. ~/.alter) ada sebelum tulis vault / state Tor.
    if let Some(parent) = args.vault_path.parent() {
        if !parent.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let online = !args.offline;

    // Setup global untuk Tor (idempotent). Dilakukan sekali sebelum bootstrap.
    if online {
        // rustls perlu CryptoProvider default terpasang sebelum dipakai arti.
        let _ = rustls::crypto::ring::default_provider().install_default();
    }

    // Subcommand `id`: butuh onion sinkron (bootstrap dulu bila online).
    if args.print_id {
        let onion = if online {
            let (cache_dir, state_dir) = tor_dirs(&args.vault_path);
            eprintln!("Bootstrap Tor untuk ambil onion address (~30-60 dtk)…");
            match TorContext::launch(&cache_dir, &state_dir, "alter-room", &[]).await {
                Ok(ctx) => Some(ctx.onion_address.clone()),
                Err(e) => {
                    eprintln!("Tor gagal: {e} — invite jadi LAN-only.");
                    None
                }
            }
        } else {
            None
        };
        let bundle = load_or_create_vault(&args.vault_path)?;
        let keys = build_self_keys(&bundle, onion.as_deref());
        let fp_grouped = keys
            .fingerprint
            .as_bytes()
            .chunks(8)
            .map(|c| std::str::from_utf8(c).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" · ");
        let transport = if onion.is_some() { "LAN + Tor" } else { "LAN" };
        println!();
        println!("  ALTER  —  secure · serverless · sovereign");
        println!("  ─────────────────────────────────────────────");
        println!();
        println!("  Invite code:");
        println!("  {}", keys.invite);
        println!();
        println!("  Fingerprint:  {fp_grouped}");
        println!("  Transport:    {transport}");
        println!("  Vault:        {}", args.vault_path.display());
        println!();
        if onion.is_none() {
            println!("  (LAN-only — mode --offline. Tanpa --offline, invite menyertakan onion.)");
            println!();
        }
        return Ok(());
    }

    // Pra-muat kontak dari CLI (opsional) — tidak butuh unlock.
    let mut contact_list: Vec<Contact> = Vec::new();
    if let Some(code) = &args.add_invite {
        match contacts::decode_invite(code) {
            Ok((ed, noise, cap, onion)) => {
                let nickname = args
                    .add_name
                    .clone()
                    .unwrap_or_else(|| format!("peer-{}", &contacts::fingerprint(&ed)[..8]));
                contact_list.push(Contact {
                    nickname,
                    ed25519_pub: ed,
                    noise_pub: noise,
                    onion,
                    tor_client_auth_pub: cap,
                });
            }
            Err(_) => {
                eprintln!("Peringatan: invite code di --add tidak valid (format v1 tidak didukung), dilewati.");
            }
        }
    }

    // Bootstrap Tor di LATAR BELAKANG (tidak memblok startup TUI). Hasilnya
    // dikirim ke TUI lewat channel; badge berubah TOR+LAN saat siap.
    let tor_rx = if online {
        let (cache_dir, state_dir) = tor_dirs(&args.vault_path);
        let (tx, rx) = mpsc::unbounded_channel::<Result<Arc<TorContext>, String>>();
        tokio::spawn(async move {
            let result = TorContext::launch(&cache_dir, &state_dir, "alter-room", &[])
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(result);
        });
        Some(rx)
    } else {
        None
    };

    // Unlock/create vault terjadi DI DALAM TUI.
    let vault_exists = args.vault_path.exists();
    tui::run(
        args.vault_path,
        vault_exists,
        args.connect,
        contact_list,
        tor_rx,
    )
    .await
}

fn main() {
    // Dev convenience di Windows: lewati cek permission fs-mistrust.
    // WAJIB sebelum tokio runtime di-build — set_var unsafe di multi-thread (Rust ≥ 1.83).
    if std::env::var("FS_MISTRUST_DISABLE_PERMISSIONS_CHECKS").is_err() {
        std::env::set_var("FS_MISTRUST_DISABLE_PERMISSIONS_CHECKS", "true");
    }

    let args = match Args::parse() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            std::process::exit(2);
        }
    };

    // SEC-09: set nama proses sebelum runtime dibuat (single-thread, aman).
    if let Some(ref name) = args.process_name {
        platform::set_process_name(name);
    }

    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("Gagal membuat runtime: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = rt.block_on(real_main(args)) {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
