# Plan: TermTube — Reproductor TUI de YouTube con visualizador

## TL;DR
Crear **TermTube**, un reproductor de música de YouTube en terminal escrito en Rust con ratatui, que incluye un visualizador estilo Winamp (barras de ecualizador), precarga de 1-2 canciones para reproducción fluida, controles completos (shuffle, repeat, favoritos, colas, edición de playlists), y soporte para archivos de configuración txt (cookies, playlists) más un config general TOML.

## Decisiones Tomadas
- **Lenguaje:** Rust con ratatui/crossterm
- **Audio:** Streaming via yt-dlp → decodificación con symphonia → salida con cpal
- **Visualizador:** Barras verticales clásicas (ecualizador FFT)
- **Precarga:** 1-2 canciones adelante (buffer en /tmp/)
- **Plataforma:** Solo Linux
- **Repo:** Nuevo repo separado (`termtube`)
- **Controles:** Completos (play/pause, next/prev, shuffle, repeat, buscar, favoritos, colas, editar playlists)
- **Config:** cookies.txt + playlist.txt + config.toml (temas, keybindings, caché)

---

## Arquitectura

### Módulos principales

```
termtube/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Entry point, setup tokio + ratatui
│   ├── app.rs                  # Estado global de la aplicación
│   ├── config/
│   │   ├── mod.rs
│   │   ├── settings.rs         # Parsing config.toml
│   │   ├── cookies.rs          # Parsing/validación cookies.txt
│   │   └── playlist.rs         # Parsing playlist.txt (formato name|url)
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── engine.rs           # Motor de audio: yt-dlp → symphonia → cpal
│   │   ├── preloader.rs        # Precarga de 1-2 canciones en background
│   │   └── queue.rs            # Cola de reproducción, shuffle, repeat
│   ├── visualizer/
│   │   ├── mod.rs
│   │   └── spectrum.rs         # FFT sobre PCM samples → barras de frecuencia
│   ├── playlist/
│   │   ├── mod.rs
│   │   ├── manager.rs          # CRUD playlists, sync con yt-dlp
│   │   ├── favorites.rs        # Gestión de favoritos
│   │   └── models.rs           # Structs: Playlist, Song, PlaylistIndex
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── layout.rs           # Layout principal de paneles
│   │   ├── playlist_view.rs    # Panel de lista de playlists/canciones
│   │   ├── now_playing.rs      # Panel "ahora suena" + progreso
│   │   ├── visualizer_view.rs  # Panel del visualizador de barras
│   │   ├── controls.rs         # Barra de controles inferior
│   │   ├── queue_view.rs       # Panel de cola de reproducción
│   │   └── theme.rs            # Temas de colores configurables
│   ├── input/
│   │   ├── mod.rs
│   │   └── handler.rs          # Keybindings y manejo de input
│   └── sync/
│       ├── mod.rs
│       └── fetcher.rs          # Fetch de metadata de playlists via yt-dlp
├── config/
│   └── default.toml            # Config por defecto
├── README.md
└── .gitignore
```

### Flujo de Audio (pipeline)

1. **yt-dlp** descarga audio stream (best audio format) → stdout pipe
2. **symphonia** decodifica el stream a PCM samples en Rust
3. PCM samples se envían a dos destinos en paralelo:
   - **cpal** → salida de audio al dispositivo del sistema
   - **Visualizador** → buffer circular de samples → FFT (rustfft) → barras de frecuencia
4. **Preloader** en background descarga 1-2 canciones siguientes a archivos temporales en disco

### Layout TUI

```
┌──────────────────────────────────────────────────────┐
│  TermTube                                    [🔀][🔁] │
├──────────────────┬───────────────────────────────────┤
│  Playlists       │  ▶ Now Playing                    │
│  ─────────       │  "Lofi beats to chill"            │
│  > lofi-beats    │  Artist - 03:24 / 05:10           │
│    synthwave     │  ████████████░░░░░░░░░            │
│    favorites ♥   │                                   │
│                  │  ┌─ Visualizer ──────────────────┐ │
│  Songs           │  │  █     █                      │ │
│  ─────────       │  │  █ █   █ █     █              │ │
│  ▶ Track 1       │  │  █ █ █ █ █ █   █ █            │ │
│    Track 2       │  │  █ █ █ █ █ █ █ █ █ █          │ │
│    Track 3       │  │  █ █ █ █ █ █ █ █ █ █ █ █      │ │
│    Track 4       │  └──────────────────────────────┘ │
├──────────────────┴───────────────────────────────────┤
│  ◀◀  ▶  ▶▶  🔀  🔁  Vol: ████░░  [q]uit [?]help    │
└──────────────────────────────────────────────────────┘
```

---

## Fases de Implementación

### Fase 1: Scaffolding y Config (fundamento)
1. Inicializar proyecto Rust con `cargo init termtube`
2. Definir dependencias en Cargo.toml: `ratatui`, `crossterm`, `tokio`, `serde`, `toml`, `symphonia`, `cpal`, `rustfft`
3. Implementar parsers de configuración:
   - `config/playlist.rs`: Parsear formato `name|url` de playlist.txt
   - `config/cookies.rs`: Validar existencia y formato Netscape de cookies.txt
   - `config/settings.rs`: Parsear config.toml (tema, keybindings, directorio de caché, ruta cookies/playlists)
4. Crear `config/default.toml` con valores por defecto
5. Implementar struct `App` en app.rs como estado central

**Verificación:** Tests unitarios para parsers de config. `cargo test` pasa.

### Fase 2: Sync de Playlists (*paralelo con Fase 3*)
6. Implementar `sync/fetcher.rs`: Ejecutar yt-dlp como proceso hijo para obtener metadata JSON de playlists
7. Implementar `playlist/models.rs`: Structs `Song { title, video_id, duration }`, `Playlist { name, songs }`, `PlaylistIndex`
8. Implementar `playlist/manager.rs`: Cargar playlists desde archivos JSON cacheados en `~/.termtube/playlists/`, sync con yt-dlp, generar índice
9. Implementar `playlist/favorites.rs`: Persistir favoritos en `~/.termtube/favorites.json`

**Verificación:** Ejecutar sync con una playlist real, verificar JSON generado.

### Fase 3: Motor de Audio (*paralelo con Fase 2*)
10. Implementar `audio/engine.rs`:
    - Spawn yt-dlp: `yt-dlp --cookies <path> -f bestaudio -o - <url>` → pipe stdout
    - Decodificar con symphonia (FormatReader → Decoder → PCM samples)
    - Output a cpal (AudioStream con callback que consume samples del buffer)
    - Exponer buffer circular de samples para el visualizador
11. Implementar `audio/queue.rs`: Cola de reproducción con modos shuffle y repeat (none/one/all)
12. Implementar `audio/preloader.rs`:
    - Background task (tokio::spawn) que descarga audio de las siguientes 1-2 canciones a `/tmp/termtube/`
    - Cuando una canción termina, la siguiente ya está en disco → reproducción inmediata
    - Limpieza de archivos temporales de canciones ya reproducidas

**Verificación:** Reproducir una canción completa desde YouTube con audio audible. Verificar que la siguiente canción inicia sin delay.

### Fase 4: TUI Base
13. Implementar `ui/layout.rs`: Layout principal con paneles (ratatui::layout::Layout)
14. Implementar `ui/theme.rs`: Sistema de temas con colores configurables desde config.toml
15. Implementar `ui/playlist_view.rs`: Lista navegable de playlists y canciones (StatefulList)
16. Implementar `ui/now_playing.rs`: Información de canción actual + barra de progreso (Gauge)
17. Implementar `ui/controls.rs`: Barra inferior con estado de controles y ayuda de teclas
18. Implementar `input/handler.rs`: Mapeo de teclas configurable (config.toml) → acciones de la app

**Verificación:** Navegar playlists, seleccionar canción, ver progreso. Controles de teclado funcionales.

### Fase 5: Visualizador (*depende de Fase 3 y 4*)
19. Implementar `visualizer/spectrum.rs`:
    - Consumir samples del buffer circular del audio engine
    - Aplicar FFT (rustfft) sobre ventanas de ~1024-2048 samples
    - Mapear frecuencias a N barras (configurable, ~16-32 barras)
    - Suavizado temporal (decay) para que las barras no salten bruscamente
    - Escala logarítmica en frecuencia (como el oído humano)
20. Implementar `ui/visualizer_view.rs`:
    - Renderizar barras como bloques Unicode (▁▂▃▄▅▆▇█) o BarChart de ratatui
    - Colores degradados por altura (verde → amarillo → rojo, configurable por tema)
    - Refresh rate ~30fps (tick cada ~33ms)

**Verificación:** Reproducir música y verificar visualmente que las barras responden al audio en tiempo real.

### Fase 6: Cola y Favoritos (*depende de Fase 3 y 4*)
21. Implementar `ui/queue_view.rs`: Panel de cola con drag-to-reorder (mover con teclas)
22. Integrar favoritos: Toggle con tecla, persistencia en disco, playlist virtual "Favoritos"
23. Edición de playlists inline: Añadir/quitar canciones de playlists, reordenar

**Verificación:** Añadir canciones a cola y favoritos, verificar persistencia al reiniciar.

### Fase 7: Pulido y CLI
24. CLI con argumentos: `termtube [--cookies <path>] [--playlists <path>] [--config <path>] [--sync]`
25. Primera ejecución: Wizard interactivo si no existe configuración
26. Manejo de errores robusto: yt-dlp no instalado, cookies expiradas, sin conexión, playlist privada
27. Logging a archivo (`~/.termtube/termtube.log`)
28. README.md completo con instrucciones de instalación y uso

**Verificación:** Test end-to-end completo: sync → navegar → reproducir → visualizar → favoritos → cerrar → reabrir con estado persistido.

---

## Dependencias Rust (Cargo.toml)

- `ratatui = "0.29"` — Framework TUI
- `crossterm = "0.28"` — Backend de terminal
- `tokio = { version = "1", features = ["full"] }` — Runtime async
- `symphonia = { version = "0.5", features = ["all"] }` — Decodificador de audio
- `cpal = "0.15"` — Output de audio
- `rustfft = "6"` — FFT para visualizador
- `serde = { version = "1", features = ["derive"] }` — Serialización
- `toml = "0.8"` — Parsing config
- `serde_json = "1"` — Parsing JSON de yt-dlp
- `clap = { version = "4", features = ["derive"] }` — CLI args
- `dirs = "5"` — Rutas estándar (~/.config, etc.)
- `rand = "0.8"` — Shuffle
- `tracing = "0.1"` + `tracing-subscriber` — Logging

---

## Archivos de Configuración

### playlist.txt (compatible con shb-yt-playlist-sync)
```
lofi-beats|https://www.youtube.com/playlist?list=PLxxxxxx
synthwave|https://music.youtube.com/playlist?list=PLyyyyyy
```

### cookies.txt (formato Netscape estándar)
Exportado con extensión "Get cookies.txt LOCALLY" desde Chromium.

### config.toml
```toml
[general]
cache_dir = "~/.termtube/cache"
log_file = "~/.termtube/termtube.log"
preload_count = 2

[paths]
cookies = "./cookies.txt"
playlists = "./playlist.txt"

[theme]
name = "default"
primary = "#61afef"
secondary = "#c678dd"
visualizer_colors = ["#98c379", "#e5c07b", "#e06c75"]

[keybindings]
play_pause = "space"
next = "n"
previous = "p"
volume_up = "+"
volume_down = "-"
shuffle = "s"
repeat = "r"
favorite = "f"
quit = "q"
search = "/"
queue = "Q"

[visualizer]
bars = 24
fps = 30
decay = 0.85
```

---

## Estructura de Datos en Disco

```
~/.termtube/
├── config.toml              # Config general (si no se pasa por CLI)
├── playlists.json           # Índice de playlists
├── favorites.json           # Lista de video_ids favoritos
├── termtube.log             # Log de la aplicación
├── playlists/
│   ├── lofi-beats.json      # Metadata cacheada por playlist
│   └── synthwave.json
└── cache/                   # Audio precargado (temporal)
    ├── dQw4w9WgXcQ.opus
    └── ...
```

---

## Requisitos del Sistema
- Linux con terminal compatible con Unicode y 256 colores
- yt-dlp instalado y en PATH
- Dispositivo de audio funcional (ALSA/PulseAudio/PipeWire)
- Conexión a internet para sync y streaming
- (Opcional) Chromium + extensión para exportar cookies

---

## Scope Explícito

### Incluido
- Reproducción de audio de YouTube/YouTube Music via streaming
- Visualizador de barras de ecualizador en tiempo real
- Precarga de 1-2 canciones siguientes
- Gestión de playlists (sync, crear, editar, eliminar)
- Favoritos persistentes
- Cola de reproducción editable
- Configuración via archivos txt + TOML
- CLI con argumentos para rutas de config
- Temas de colores configurables

### Excluido (fuera de scope v1)
- Soporte Windows/macOS
- Descarga permanente de audio (solo caché temporal)
- Búsqueda de canciones en YouTube (solo playlists predefinidas)
- Soporte de otros servicios (Spotify, SoundCloud, etc.)
- Letras de canciones
- Ecualizador de audio (solo visualizador)
- Interfaz web o GUI
