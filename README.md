# ymz (Yandex Music Zero)

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-dea584?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Platform-Arch_Linux-1793d1?style=for-the-badge&logo=arch-linux" alt="Arch Linux" />
  <img src="https://img.shields.io/badge/Memory-~18MB_RSS-brightgreen?style=for-the-badge" alt="RAM" />
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="License" />
</p>

Ультралегковесный headless-клиент для **Яндекс Музыки**, написанный на Rust. Играет исключительно поток **«Моя волна»**, не тащит за собой Electron/Chromium, нативно интегрируется в окружение через **MPRIS v2** и управляется стандартными средствами системы (`playerctl`, Waybar, виджеты панелей, медиаклавиши).
Признаюсь, был написан ии за пару минут, но контролировался человеком.

---

### Особенности

* **Zero-bloat:** потребление памяти в пределах **15–25 МБ RAM** (против ~700 МБ у официального клиента).
* **Нативный MPRIS v2:** полная поддержка D-Bus сигналов `PropertiesChanged` — название, исполнитель и обложка трека отображаются в системных панелях в реальном времени.
* **Честная «Моя волна»:** отправка сетевого фидбека (`trackStarted`, `trackFinished`, `skip`) на серверы Яндекса для корректной работы алгоритмов рекомендаций.
* **Нативный звук:** стриминг и декодирование аудио через `rodio` / `symphonia` напрямую в PipeWire/ALSA.

---

### Зависимости

На Arch Linux:

```bash
sudo pacman -S alsa-lib pkgconf base-devel

```

---

### Сборка и установка

```bash
git clone https://github.com/BBQQYT/ymz.git
cd ymz
cargo build --release

```

Бинарник с оптимизацией по размеру появится в `./target/release/ymz`.

---

### Настройка и запуск

#### 1. Получение токена

Для работы плеера требуется OAuth-токен аккаунта Яндекс Музыки (`YM_TOKEN`).

#### 2. Запуск через systemd (user service)

Создайте юнит `~/.config/systemd/user/ymz.service`:

```ini
[Unit]
Description=Yandex Music Zero Daemon
After=pipewire.service wireplumber.service sound.target

[Service]
Type=simple
ExecStart=%h/ymz/target/release/ymz
Environment=YM_TOKEN=y0_AgAAAA...ВАШ_ТОКЕН...
Restart=always
RestartSec=5

[Install]
WantedBy=default.target

```

Запуск и активация:

```bash
systemctl --user daemon-reload
systemctl --user enable --now ymz.service

```

---

### Управление

Управление воспроизведением через терминал или бинды тайловых WM (`hyprland.conf`, `rc.lua`, `i3/config`):

```bash
# Пауза / Плей
playerctl -p ymz play-pause

# Следующий трек (с отправкой skip-фидбека)
playerctl -p ymz next

# Текущий статус
playerctl -p ymz metadata

```

#### Waybar config:

```jsonc
"mpris": {
    "player": "ymz",
    "format": "{status_icon} {artist} — {title}",
    "status-icons": {
        "playing": "▶",
        "paused": "⏸"
    }
}

```

---

### Лицензия

MIT

