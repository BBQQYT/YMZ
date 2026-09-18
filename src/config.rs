use std::env;
use std::fs;
use std::path::PathBuf;

pub fn load_token() -> Result<String, String> {
    // 1. Проверяем ~/.config/ymz/token
    if let Ok(home) = env::var("HOME") {
        let path = PathBuf::from(home).join(".config/ymz/token");
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                let trimmed = content.trim().to_string();
                if !trimmed.is_empty() {
                    return Ok(trimmed);
                }
            }
        }
    }

    // 2. Фолбэк на переменную окружения
    if let Ok(token) = env::var("YM_TOKEN") {
        let trimmed = token.trim().to_string();
        if !trimmed.is_empty() {
            return Ok(trimmed);
        }
    }

    Err("Токен не найден! Создайте ~/.config/ymz/token с правами 600 или передайте YM_TOKEN".into())
}
