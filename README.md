# YMZ (Yandex Music Zero)

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-dea584?style=for-the-badge&logo=rust" alt="Rust" />
  <img src="https://img.shields.io/badge/Platform-Linux-1793d1?style=for-the-badge&logo=linux" alt="Linux" />
  <img src="https://img.shields.io/badge/Memory-~18MB_RSS-brightgreen?style=for-the-badge" alt="RAM" />
  <img src="https://img.shields.io/badge/License-MIT-blue?style=for-the-badge" alt="License" />
</p>

Ультралегковесный headless-клиент для **Яндекс Музыки**, написанный на Rust. Играет исключительно поток **«Моя волна»**, не тащит Electron/Chromium, нативно интегрируется в окружение через **MPRIS v2** и управляется стандартными системными средствами (`playerctl`, виджеты панелей, медиаклавиши).

Да, написан ИИ, я этого не скрываю!

---

### Особенности

* **Zero-bloat:** потребление памяти в пределах **15–20 МБ RSS** (против ~700 МБ у десктопного веб-клиента).
* **Полноценный MPRIS v2:**
  * Название, артист и обложка трека в реальном времени.
  * Синхронизация времени: отображение длительности (`mpris:length`) и шкалы воспроизведения (`Position`).
  * Полная поддержка перемотки по клику на ползунок (`Seek`, `SetPosition`).
* **Безопасность:** изолированное хранение OAuth-токена в `~/.config/ymz/token` с правами доступа `600` (токен не светится в процессах и логах).
* **Сетевая устойчивость:** автоматический retry с экспоненциальной задержкой при сбоях сети или кратковременных обрывах связи.
* **Честный ротор:** корректная отправка сетевого фидбека (`trackStarted`, `trackFinished`, `skip`) в алгоритмы Яндекса.
* **Универсальность:** работает с PipeWire, PulseAudio и чистой ALSA на любых дистрибутивах Linux.

---

### Системные зависимости

Для сборки требуются заголовочные файлы ALSA и `pkg-config`:

* **Arch Linux / Manjaro / CachyOS:**
  ```bash
  sudo pacman -S alsa-lib pkgconf base-devel

  ```

  * **Ubuntu / Debian / Linux Mint / Pop!_OS:**
  ```bash
  sudo apt install libasound2-dev pkg-config build-essential
  ```


* **Fedora / RHEL / AlmaLinux:**
  ```bash
  sudo dnf install alsa-lib-devel pkgconf-pkg-config gcc
  ```


* **openSUSE (Tumbleweed / Leap):**
  ```bash
  sudo zypper install alsa-devel pkg-config gcc

  ```


* **Void Linux:**
  ```bash
  sudo xbps-install -S alsa-lib-devel base-devel

  ```



---

### Сборка и установка

1. **Клонирование репозитория:**
  ```bash
  git clone https://github.com/BBQQYT/YMZ.git
  cd YMZ

  ```


2. **Компиляция релизного бинарника:**
  ```bash

  git clone https://github.com/BBQQYT/ymz.git
  cd ymz
  cargo build --release

  ```


3. **(Опционально) Установка в систему:**
  ```bash
  sudo install -Dm755 target/release/ymz /usr/local/bin/ymz

  ```



---

### Настройка

Сохраните OAuth-токен Яндекс Музыки в конфигурационный файл и ограничьте права доступа:

```bash
mkdir -p ~/.config/ymz
echo "ВАШ_ТОКЕН" > ~/.config/ymz/token
chmod 600 ~/.config/ymz/token

```

> Также поддерживается передача токена через переменную окружения `YM_TOKEN`.

---

### Автозапуск

#### Вариант 1: systemd user service (рекомендуется)

Создайте файл `~/.config/systemd/user/ymz.service`:

```ini
[Unit]
Description=Yandex Music Zero Daemon
After=pipewire.service wireplumber.service sound.target

[Service]
Type=simple
ExecStart=/usr/local/bin/ymz
Restart=always
RestartSec=3

[Install]
WantedBy=default.target

```

Активация и запуск:

```bash
systemctl --user daemon-reload
systemctl --user enable --now ymz.service

```

#### Вариант 2: Запуск без systemd (Hyprland / Sway / AwesomeWM / i3)

Просто добавьте вызов бинарника в конфиг оконного менеджера:

* **Hyprland** (`hyprland.conf`):
```ini
exec-once = ymz

```


* **Sway** (`config`):
```ini
exec ymz

```


* **AwesomeWM** (`rc.lua`):
```lua
awful.spawn.with_shell("ymz")

```



---

### Управление

```bash
# Плей / Пауза
playerctl -p ymz play-pause

# Следующий трек (с отправкой skip-статистики)
playerctl -p ymz next

# Перемотка вперед/назад на 10 секунд
playerctl -p ymz position 10+
playerctl -p ymz position 10-

# Перейти на конкретную секунду (например, 1:15)
playerctl -p ymz position 75

# Текущие метаданные и статус
playerctl -p ymz metadata

```

#### Интеграция с Waybar (`config.jsonc`):

```jsonc
"mpris": {
    "player": "ymz",
    "format": "{player_icon} {artist} — {title} [{position}/{length}]",
    "player-icons": {
        "default": "▶",
        "playing": "▶",
        "paused": "⏸"
    }
}

```

---

### Лицензия

Проект распространяется под лицензией [MIT](https://www.google.com/search?q=LICENSE&utm_source=gemini).


