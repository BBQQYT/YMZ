use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use rodio::{Decoder, OutputStream, Sink};
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use zbus::zvariant::{ObjectPath, Value};
use zbus::{connection::Builder, interface, Connection};

const BASE_URL: &str = "https://api.music.yandex.net";

#[derive(Deserialize, Debug)]
struct StationTracksResponse {
    result: StationResult,
}

#[derive(Deserialize, Debug)]
struct StationResult {
    sequence: Vec<TrackEntry>,
    #[serde(rename = "batchId")]
    batch_id: String,
}

#[derive(Deserialize, Debug)]
struct TrackEntry {
    track: Track,
}

#[derive(Deserialize, Debug, Clone)]
struct Track {
    id: String,
    title: String,
    artists: Vec<Artist>,
    #[serde(rename = "coverUri")]
    cover_uri: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
struct Artist {
    name: String,
}

#[derive(Deserialize, Debug)]
struct DownloadInfoResponse {
    result: Vec<DownloadInfo>,
}

#[derive(Deserialize, Debug)]
struct DownloadInfo {
    codec: String,
    #[serde(rename = "downloadInfoUrl")]
    download_info_url: String,
}

pub struct YandexMusicClient {
    client: reqwest::Client,
}

impl YandexMusicClient {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token)).expect("Invalid token"),
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YandexMusicAndroid/24022371"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        Self { client }
    }

    pub async fn get_wave_tracks(&self) -> Result<StationResult, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/rotor/station/user:onyourwave/tracks", BASE_URL);
        let res = self.client.get(&url).send().await?.json::<StationTracksResponse>().await?;
        Ok(res.result)
    }

    pub async fn send_feedback(&self, batch_id: &str, track_id: &str, event_type: &str) {
        let url = format!("{}/rotor/station/user:onyourwave/feedback", BASE_URL);
        let payload = serde_json::json!({
            "type": event_type,
            "trackId": track_id,
            "batchId": batch_id,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });
        let _ = self.client.post(&url).json(&payload).send().await;
    }

    pub async fn get_stream_url(&self, track_id: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let info_url = format!("{}/tracks/{}/download-info", BASE_URL, track_id);
        let info_res = self.client.get(&info_url).send().await?.json::<DownloadInfoResponse>().await?;

        let target = info_res
            .result
            .into_iter()
            .find(|d| d.codec == "mp3")
            .ok_or("MP3 stream not found")?;

        let xml_raw = self.client.get(&target.download_info_url).send().await?.text().await?;
        let doc = roxmltree::Document::parse(&xml_raw)?;

        let host = doc.descendants().find(|n| n.has_tag_name("host")).unwrap().text().unwrap();
        let path = doc.descendants().find(|n| n.has_tag_name("path")).unwrap().text().unwrap();
        let ts = doc.descendants().find(|n| n.has_tag_name("ts")).unwrap().text().unwrap();
        let sign = doc.descendants().find(|n| n.has_tag_name("s")).unwrap().text().unwrap();

        let salt = "XGRlBW9FXlekgbPrr";
        let mut hasher = Md5::new();
        hasher.update(format!("{}{}{}", salt, &path[1..], sign).as_bytes());
        let hash = format!("{:x}", hasher.finalize());

        Ok(format!("https://{}/get-mp3/{}/{}{}", host, hash, ts, path))
    }
}

enum PlayerCommand {
    PlayPause,
    Next,
    Stop,
}

struct MprisRoot;

#[interface(name = "org.mpris.MediaPlayer2")]
impl MprisRoot {
    #[zbus(property)]
    fn can_quit(&self) -> bool { true }

    #[zbus(property)]
    fn can_raise(&self) -> bool { false }

    #[zbus(property)]
    fn has_track_list(&self) -> bool { false }

    #[zbus(property)]
    fn identity(&self) -> &str { "Yandex Music Zero" }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> { vec![] }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> { vec!["audio/mpeg".into()] }
}

struct MprisPlayer {
    cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    sink: Arc<Sink>,
    current_title: Arc<RwLock<String>>,
    current_artist: Arc<RwLock<String>>,
    current_art_url: Arc<RwLock<String>>,
    current_track_id: Arc<RwLock<String>>,
}

#[interface(name = "org.mpris.MediaPlayer2.Player")]
impl MprisPlayer {
    async fn next(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Next);
    }

    async fn play_pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayPause);
    }

    async fn pause(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayPause);
    }

    async fn play(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::PlayPause);
    }

    async fn stop(&self) {
        let _ = self.cmd_tx.send(PlayerCommand::Stop);
    }

    #[zbus(property)]
    fn playback_status(&self) -> &str {
        if self.sink.is_paused() {
            "Paused"
        } else {
            "Playing"
        }
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool { true }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool { false }

    #[zbus(property)]
    fn can_play(&self) -> bool { true }

    #[zbus(property)]
    fn can_pause(&self) -> bool { true }

    #[zbus(property)]
    fn can_control(&self) -> bool { true }

    #[zbus(property)]
    async fn metadata(&self) -> HashMap<String, Value<'static>> {
        let title = self.current_title.read().await;
        let artist = self.current_artist.read().await;
        let art = self.current_art_url.read().await;
        let tid = self.current_track_id.read().await;

        build_metadata_map(&title, &artist, &art, &tid)
    }
}

// Создаем owned Value<'static>, чтобы избежать проблем с lifetimes
fn build_metadata_map(
    title: &str,
    artist: &str,
    art_url: &str,
    track_id: &str,
) -> HashMap<String, Value<'static>> {
    let mut m = HashMap::new();
    let sanitized_id: String = track_id.chars().filter(|c| c.is_alphanumeric() || *c == '_').collect();
    let path_str = if sanitized_id.is_empty() {
        "/org/mpris/MediaPlayer2/TrackList/NoTrack".to_string()
    } else {
        format!("/org/ymz/Track/{}", sanitized_id)
    };

    let track_path = ObjectPath::try_from(path_str)
        .unwrap_or_else(|_| ObjectPath::from_str_unchecked("/org/mpris/MediaPlayer2/TrackList/NoTrack"));

    m.insert("mpris:trackid".to_string(), Value::from(track_path));
    m.insert("xesam:title".to_string(), Value::from(title.to_string()));
    m.insert("xesam:artist".to_string(), Value::from(vec![artist.to_string()]));
    if !art_url.is_empty() {
        m.insert("mpris:artUrl".to_string(), Value::from(art_url.to_string()));
    }
    m
}

async fn notify_changed(conn: &Connection, changed: HashMap<&str, Value<'_>>) {
    let invalidated: Vec<&str> = vec![];
    let _ = conn
        .emit_signal(
            Option::<&str>::None,
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
            &("org.mpris.MediaPlayer2.Player", changed, invalidated),
        )
        .await;
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let token = std::env::var("YM_TOKEN").expect("Задайте переменную окружения YM_TOKEN");
    let ym = Arc::new(YandexMusicClient::new(&token));

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&stream_handle)?);

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<PlayerCommand>();

    let current_title = Arc::new(RwLock::new("Моя волна".to_string()));
    let current_artist = Arc::new(RwLock::new("Яндекс Музыка".to_string()));
    let current_art_url = Arc::new(RwLock::new("".to_string()));
    let current_track_id = Arc::new(RwLock::new("0".to_string()));

    let mpris_player = MprisPlayer {
        cmd_tx: cmd_tx.clone(),
        sink: sink.clone(),
        current_title: current_title.clone(),
        current_artist: current_artist.clone(),
        current_art_url: current_art_url.clone(),
        current_track_id: current_track_id.clone(),
    };

    let conn = Builder::session()?
        .name("org.mpris.MediaPlayer2.ymz")?
        .serve_at("/org/mpris/MediaPlayer2", MprisRoot)?
        .serve_at("/org/mpris/MediaPlayer2", mpris_player)?
        .build()
        .await?;

    println!("[ymz] MPRIS шина зарегистрирована как org.mpris.MediaPlayer2.ymz");

    let skip_flag = Arc::new(AtomicBool::new(false));

    let sink_ctrl = sink.clone();
    let skip_ctrl = skip_flag.clone();
    let conn_ctrl = conn.clone();

    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            match cmd {
                PlayerCommand::PlayPause => {
                    let status = if sink_ctrl.is_paused() {
                        sink_ctrl.play();
                        "Playing"
                    } else {
                        sink_ctrl.pause();
                        "Paused"
                    };
                    let mut changed = HashMap::new();
                    changed.insert("PlaybackStatus", Value::from(status));
                    notify_changed(&conn_ctrl, changed).await;
                }
                PlayerCommand::Next => {
                    skip_ctrl.store(true, Ordering::SeqCst);
                    sink_ctrl.stop();
                }
                PlayerCommand::Stop => {
                    sink_ctrl.stop();
                    let mut changed = HashMap::new();
                    changed.insert("PlaybackStatus", Value::from("Stopped"));
                    notify_changed(&conn_ctrl, changed).await;
                }
            }
        }
    });

    loop {
        let station = match ym.get_wave_tracks().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[ymz] Ошибка получения волны: {e}, повтор через 5 сек");
                sleep(Duration::from_secs(5)).await;
                continue;
            }
        };

        for entry in station.sequence {
            let track = entry.track;
            let artist_name = track
                .artists
                .first()
                .map(|a| a.name.clone())
                .unwrap_or_else(|| "Неизвестный исполнитель".to_string());

            let cover_url = track
                .cover_uri
                .as_ref()
                .map(|uri| format!("https://{}", uri.replace("%%", "400x400")))
                .unwrap_or_default();

            *current_title.write().await = track.title.clone();
            *current_artist.write().await = artist_name.clone();
            *current_track_id.write().await = track.id.clone();
            *current_art_url.write().await = cover_url.clone();

            let meta = build_metadata_map(&track.title, &artist_name, &cover_url, &track.id);
            let mut changed = HashMap::new();
            changed.insert("Metadata", Value::from(meta));
            changed.insert("PlaybackStatus", Value::from("Playing"));
            notify_changed(&conn, changed).await;

            println!("[ymz] ▶ {} — {}", artist_name, track.title);

            let stream_url = match ym.get_stream_url(&track.id).await {
                Ok(url) => url,
                Err(_) => continue,
            };

            ym.send_feedback(&station.batch_id, &track.id, "trackStarted").await;

            if let Ok(resp) = reqwest::get(&stream_url).await {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                        skip_flag.store(false, Ordering::SeqCst);
                        sink.append(source);
                        sink.play();

                        while !sink.empty() {
                            if skip_flag.load(Ordering::SeqCst) {
                                ym.send_feedback(&station.batch_id, &track.id, "skip").await;
                                break;
                            }
                            sleep(Duration::from_millis(200)).await;
                        }

                        if !skip_flag.load(Ordering::SeqCst) {
                            ym.send_feedback(&station.batch_id, &track.id, "trackFinished").await;
                        }
                    }
                }
            }
        }
    }
}
