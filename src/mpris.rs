use rodio::Sink;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use zbus::zvariant::{ObjectPath, Value};
use zbus::{interface, Connection};

#[derive(Debug)]
pub enum PlayerCommand {
    PlayPause,
    Next,
    Stop,
}

pub struct MprisRoot;

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

pub struct MprisPlayer {
    pub cmd_tx: mpsc::UnboundedSender<PlayerCommand>,
    pub sink: Arc<Sink>,
    pub current_title: Arc<RwLock<String>>,
    pub current_artist: Arc<RwLock<String>>,
    pub current_art_url: Arc<RwLock<String>>,
    pub current_track_id: Arc<RwLock<String>>,
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
        } else if self.sink.empty() {
            "Stopped"
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

pub fn build_metadata_map(
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

pub async fn notify_changed(conn: &Connection, changed: HashMap<&str, Value<'_>>) {
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
