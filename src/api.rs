use md5::{Digest, Md5};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde::Deserialize;
use std::time::Duration;
use tokio::time::sleep;

const BASE_URL: &str = "https://api.music.yandex.net";

#[derive(Deserialize, Debug)]
struct StationTracksResponse {
    result: StationResult,
}

#[derive(Deserialize, Debug, Clone)]
pub struct StationResult {
    pub sequence: Vec<TrackEntry>,
    #[serde(rename = "batchId")]
    pub batch_id: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct TrackEntry {
    pub track: Track,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artists: Vec<Artist>,
    #[serde(rename = "coverUri")]
    pub cover_uri: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Artist {
    pub name: String,
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

#[derive(Clone)]
pub struct YandexClient {
    client: reqwest::Client,
}

impl YandexClient {
    pub fn new(token: &str) -> Self {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("OAuth {}", token)).expect("Некорректный токен"),
        );
        headers.insert(
            "X-Yandex-Music-Client",
            HeaderValue::from_static("YandexMusicAndroid/24022371"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(Duration::from_secs(10))
            .build()
            .unwrap();

        Self { client }
    }

    // Запрос треков станции с автоматическим retry
    pub async fn get_wave_tracks(&self) -> Result<StationResult, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/rotor/station/user:onyourwave/tracks", BASE_URL);
        let mut retries = 3;
        let mut delay = Duration::from_secs(2);

        loop {
            match self.client.get(&url).send().await {
                Ok(resp) if resp.status().is_success() => {
                    let data = resp.json::<StationTracksResponse>().await?;
                    return Ok(data.result);
                }
                Ok(resp) => {
                    log::warn!("API ответил со статусом {}. Повтор...", resp.status());
                }
                Err(e) => {
                    log::warn!("Сетевая ошибка при запросе волны: {}. Повтор...", e);
                }
            }

            retries -= 1;
            if retries == 0 {
                return Err("Превышено количество попыток подключения к API".into());
            }

            sleep(delay).await;
            delay *= 2;
        }
    }

    pub async fn send_feedback(&self, batch_id: &str, track_id: &str, event_type: &str) {
        let url = format!("{}/rotor/station/user:onyourwave/feedback", BASE_URL);
        let payload = serde_json::json!({
            "type": event_type,
            "trackId": track_id,
            "batchId": batch_id,
            "timestamp": chrono::Utc::now().to_rfc3339()
        });

        // Feedback шлется fire-and-forget, не блокируя поток
        let client = self.client.clone();
        tokio::spawn(async move {
            let _ = client.post(&url).json(&payload).send().await;
        });
    }

    pub async fn get_stream_url(&self, track_id: &str) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let info_url = format!("{}/tracks/{}/download-info", BASE_URL, track_id);
        let info_res = self.client.get(&info_url).send().await?.json::<DownloadInfoResponse>().await?;

        let target = info_res
            .result
            .into_iter()
            .find(|d| d.codec == "mp3")
            .ok_or("MP3 поток не найден")?;

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
