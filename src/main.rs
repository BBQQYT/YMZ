mod api;
mod config;
mod mpris;

use api::YandexClient;
use crate::mpris::{build_metadata_map, notify_changed, notify_seeked, MprisPlayer, MprisRoot, PlayerCommand};
use rodio::{Decoder, OutputStream, Sink};
use std::collections::HashMap;
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::{mpsc, RwLock};
use tokio::time::{sleep, Duration};
use zbus::connection::Builder;
use zbus::zvariant::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let token = config::load_token()?;
    let ym = Arc::new(YandexClient::new(&token));

    let (_stream, stream_handle) = OutputStream::try_default()?;
    let sink = Arc::new(Sink::try_new(&stream_handle)?);

    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<PlayerCommand>();

    let current_title = Arc::new(RwLock::new("Моя волна".to_string()));
    let current_artist = Arc::new(RwLock::new("Яндекс Музыка".to_string()));
    let current_art_url = Arc::new(RwLock::new("".to_string()));
    let current_track_id = Arc::new(RwLock::new("0".to_string()));
    let current_duration_us = Arc::new(RwLock::new(0i64));

    let mpris_player = MprisPlayer {
        cmd_tx: cmd_tx.clone(),
        sink: sink.clone(),
        current_title: current_title.clone(),
        current_artist: current_artist.clone(),
        current_art_url: current_art_url.clone(),
        current_track_id: current_track_id.clone(),
        current_duration_us: current_duration_us.clone(),
    };

    let conn = Builder::session()?
        .name("org.mpris.MediaPlayer2.ymz")?
        .serve_at("/org/mpris/MediaPlayer2", MprisRoot)?
        .serve_at("/org/mpris/MediaPlayer2", mpris_player)?
        .build()
        .await?;

    log::info!("D-Bus шина org.mpris.MediaPlayer2.ymz зарегистрирована");

    let skip_flag = Arc::new(AtomicBool::new(false));

    let sink_ctrl = sink.clone();
    let skip_ctrl = skip_flag.clone();
    let conn_ctrl = conn.clone();
    let duration_ctrl = current_duration_us.clone();

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
                PlayerCommand::Seek(offset_us) => {
                    let cur = sink_ctrl.get_pos().as_micros() as i64;
                    let total = *duration_ctrl.read().await;
                    let target = (cur + offset_us).clamp(0, total);
                    if sink_ctrl.try_seek(Duration::from_micros(target as u64)).is_ok() {
                        notify_seeked(&conn_ctrl, target).await;
                    }
                }
                PlayerCommand::SetPosition(pos_us) => {
                    let total = *duration_ctrl.read().await;
                    let target = pos_us.clamp(0, total);
                    if sink_ctrl.try_seek(Duration::from_micros(target as u64)).is_ok() {
                        notify_seeked(&conn_ctrl, target).await;
                    }
                }
            }
        }
    });

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    log::info!("Запуск воспроизведения потока «Моя волна»");

    loop {
        let station = tokio::select! {
            _ = sigterm.recv() => break,
            _ = sigint.recv() => break,
            res = ym.get_wave_tracks() => match res {
                Ok(s) => s,
                Err(e) => {
                    log::error!("Ошибка волны: {}. Повтор через 5с", e);
                    sleep(Duration::from_secs(5)).await;
                    continue;
                }
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

            let duration_us = track.duration_ms.unwrap_or(0) * 1000;

            *current_title.write().await = track.title.clone();
            *current_artist.write().await = artist_name.clone();
            *current_track_id.write().await = track.id.clone();
            *current_art_url.write().await = cover_url.clone();
            *current_duration_us.write().await = duration_us;

            let meta = build_metadata_map(&track.title, &artist_name, &cover_url, &track.id, duration_us);
            let mut changed = HashMap::new();
            changed.insert("Metadata", Value::from(meta));
            changed.insert("PlaybackStatus", Value::from("Playing"));
            notify_changed(&conn, changed).await;

            log::info!("▶ {} — {}", artist_name, track.title);

            let stream_url = match ym.get_stream_url(&track.id).await {
                Ok(url) => url,
                Err(e) => {
                    log::warn!("Ошибка получения потока: {}", e);
                    continue;
                }
            };

            ym.send_feedback(&station.batch_id, &track.id, "trackStarted").await;

            if let Ok(resp) = reqwest::get(&stream_url).await {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(source) = Decoder::new(Cursor::new(bytes)) {
                        skip_flag.store(false, Ordering::SeqCst);
                        sink.append(source);
                        sink.play();

                        loop {
                            tokio::select! {
                                _ = sigterm.recv() => return Ok(()),
                                _ = sigint.recv() => return Ok(()),
                                _ = sleep(Duration::from_millis(150)) => {
                                    if skip_flag.load(Ordering::SeqCst) {
                                        ym.send_feedback(&station.batch_id, &track.id, "skip").await;
                                        break;
                                    }
                                    if sink.empty() {
                                        ym.send_feedback(&station.batch_id, &track.id, "trackFinished").await;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    log::info!("Завершение работы ymz");
    Ok(())
}
