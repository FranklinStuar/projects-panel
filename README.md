# Panel WP

Desktop panel for local WordPress development. Rust/Tauri 2 + SvelteKit + Docker.
Replacement for LocalWP — lighter, faster, zero idle resources.

## Why

LocalWP runs all services all the time, burning RAM/CPU even with no sites active.
Panel WP inverts that model around three rules:

1. **Nothing runs unless needed** — containers only start for active projects. Stopped = 0 resources.
2. **Share before duplicating** — 1 nginx for all projects, DB per version, 1 mailpit, 1 minio.
3. **Minimal images** — Alpine where possible, no opaque background services.

## Features

- **One-click WordPress** — create sites with WP-CLI, choose PHP/MySQL versions
- **Auto-login** — open wp-admin logged in with a single click (ephemeral token)
- **Live logs** — streaming container logs with autoscroll buffer
- **SSL on `*.test`** — mkcert + dnsmasq wildcard, no browser warnings
- **Smart shared services** — nginx, DB, mailpit, minio spin up on demand and tear down when unused
- **Git worktree projects** — test a theme/plugin branch in isolation via mount overlays, no code duplication
- **Snapshots + clones** — checkpoint and fork projects for safe experimentation
- **LocalWP importer** — migrate sites from LocalWP
- **Migration between systems** — export + import with auto-dump on stop
- **Adminer** — shared DB viewer with zero-click auto-login
- **KDE Plasmoid** — system tray widget for quick start/stop
- **Dark/light theme** — Tailwind CSS with system preference detection

## Stack

| Layer | Tech |
|-------|------|
| Desktop shell | Tauri 2 (Rust) |
| Frontend | SvelteKit + Svelte 5 (SPA, `adapter-static`) |
| Styling | Tailwind CSS |
| Container runtime | Docker via bollard |
| Async | tokio |
| IPC | Tauri commands + events |
| Plasmoid | QML + D-Bus (zbus) |

## Quick start

### Prerequisites

- Rust toolchain (stable)
- Node.js 20+ + pnpm
- Docker Engine 24+ (API socket at `/var/run/docker.sock`)
- Manjaro KDE (or any Linux with WebKitGTK)

```bash
# 1. Install deps
pnpm install

# 2. First-run setup (docker network, dnsmasq wildcard, mkcert)
bash scripts/first-run.sh

# 3. Launch dev mode
pnpm tauri dev
```

On Wayland, if the window renders blank:
```bash
WEBKIT_DISABLE_DMABUF_RENDERER=1 pnpm tauri dev
```

### Build

```bash
pnpm build                   # frontend static build → build/
cd src-tauri && cargo build  # release binary
```

### Test

```bash
cd src-tauri && cargo test                # unit tests (no Docker needed)
cargo test -- --ignored --test-threads=1  # integration tests (needs Docker)
pnpm dev:mock                             # SPA with mocked IPC (frontend-only)
pnpm test:e2e                             # Playwright e2e against dev:mock
```

## Architecture

```
panel-net (Docker bridge)
├── panel-nginx         reverse proxy, 1 vhost per active project
├── panel-mysql-80      shared DB per version (on-demand)
├── panel-mariadb-1011
├── panel-mailpit       email catcher
├── panel-minio         S3-compatible storage
├── panel-adminer       DB viewer with auto-login
├── wp-project-a        php-fpm container (only when active)
├── wp-project-b        php-fpm container (only when active)
└── ...
```

Each project gets a `php-fpm` container (`wp-{id}`) that only runs while active.
Stopped project = container removed = 0 CPU, 0 RAM. DB data survives on bind-mounted
volumes (`~/panel-wp/db-data/`).

## Project structure

```
src/                   SvelteKit frontend (SPA)
src-tauri/src/         Rust backend
  docker.rs            container orchestration (bollard)
  nginx.rs             vhost generation + reload
  php.rs               php-fpm image build
  wordpress.rs         site creation (tarball, DB, wp-config, WP-CLI)
  domain.rs            dnsmasq wildcard + SSL
  backup.rs            DB export + rotation
  migrate.rs           cross-system migration
  snapshot.rs          save/restore snapshots
  clone.rs             clone projects
  worktree.rs          git worktree test projects
  localwp.rs           LocalWP importer
  groups.rs            durable group list + ordering
  autologin.rs         one-click admin tokens
  logs.rs              container log streaming
docker/                Dockerfile + templates + mu-plugins
scripts/               first-run, wp-wrapper, CLI
docs/                  architecture, extending, changelog
```

## Docs

- [`PLAN.md`](PLAN.md) — full product plan, phases, UI flows
- [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) — container/resource model, IPC catalog, data flow
- [`docs/EXTENDING.md`](docs/EXTENDING.md) — how to add features step by step
- [`docs/CHANGELOG.md`](docs/CHANGELOG.md) — what was built and when

## License

Proprietary — Gold Media Tech.
