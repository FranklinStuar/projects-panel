# Modelo de dominio y persistencia

## Fuente de verdad

Cada proyecto es una carpeta bajo `~/panel-wp/` cuyo `config.json` es su identidad durable. `config::load_all_sites` escanea carpetas, ignora entradas sin configuración y continúa ante JSON inválido registrando el error. No existe tabla maestra.

```text
~/panel-wp/{carpeta}/
├── config.json                    ← registro activo
├── config.disconnected.json       ← registro preservado, fuera del panel
├── app/public/                    ← WordPress y repos
├── app/sql/db-*.sql               ← dumps rotados
├── conf/php/php.ini
├── ssl/{cert.pem,key.pem}
├── snapshots/{id}/...             ← puntos de guardado
├── wt/{repo}/                      ← git worktree, si aplica
└── wp-config.php                   ← override de worktree-project
```

`SiteConfig` contiene identidad (`id`, nombre, ruta, dominio, grupo), servicios PHP/nginx/DB, integración Git, opciones y banderas de migración. `SiteState` agrega el `SiteStatus` calculado desde Docker, salvo `migration_pending`, que prevalece.

## Agregados relacionados

```text
SiteConfig
 ├─ Services
 │   ├─ PhpService(version)
 │   ├─ NginxService(ssl)
 │   └─ DbService(type, version, dbName)
 ├─ GithubConfig.repos[]
 ├─ cloneOf? ───────→ padre + snapshot
 └─ worktreeOf? ────→ padre + repo + rama + política DB
```

Un clone temporal copia el estado capturado por snapshot y conserva `CloneInfo`. Un worktree-project no duplica WordPress: `WorktreeInfo` referencia al padre, monta su `public`, sobrepone el repo objetivo y un `wp-config.php` propio. Con `shared_db=true` comparte esquema sin reescribir URLs de la DB; con falso utiliza una copia.

## Persistencia global

`config::config_dir` resuelve `~/.config/wordpress-panel/`. Allí viven:

- `panel.json`: `PanelConfig`, principalmente el endpoint persistido.
- `groups.json`: orden y existencia de grupos, incluidos vacíos; la pertenencia sigue en `SiteConfig.group`.
- `db-data/{container}`: datadir durable por motor+versión.
- `minio-data/`, `wp-cli.phar`, vhosts y certificados auxiliares según módulo.
- `dump-log.jsonl`: auditoría funcional de escrituras de dumps; podarlo no borra SQL.

El endpoint se elige una vez y se persiste porque WordPress guarda URLs con puerto. Aunque `Endpoint::default` modela 80/443 para compatibilidad, el selector runtime actual `DockerManager::autoselect_endpoint` **siempre** busca puertos altos desde 8080/8443.

## Reglas de identidad y estado

- Container PHP: `SiteConfig::container_name` → `wp-{id}`.
- DB compartida: `docker::db_container_name` → `panel-{motor}-{versión sin puntos}`.
- Un proyecto desconectado deja de aparecer al renombrar `config.json` a `config.disconnected.json`; sus archivos sobreviven.
- Una carpeta sin sidecar puede reconstruirse best-effort si contiene `app/public/wp-config.php`; queda `migrationPending`.
- `GithubConfig::normalize` pliega campos legacy `theme/plugins` en `repos` al leer.
- Serde usa `camelCase`; `DbService` expone `type` y `dbName`. `src/lib/types.ts` debe seguir siendo espejo exacto.

## Datos de base de datos

Una instancia DB sirve varios esquemas de proyectos compatibles. Su datadir se bindea al host mediante `docker::db_data_dir` y `DbType::datadir`, por lo que recrear el container no debe destruir datos. Containers legacy sin el bind correcto se migran una vez con `DockerManager::migrate_db_to_volume` y `docker cp`.

Los dumps son una segunda capa de recuperación: exportación manual, al detener y watcher automático. `backup::rotate_dumps` conserva una ventana corta; `dumplog::append` registra fuente `manual`, `stop` o `auto`.

## Ownership y consistencia

```text
config.json      Rust/config.rs      identidad y opciones durables
Docker daemon    DockerManager       running/existencia/imagen
DB datadir       motor DB            datos vivos, bind del host
app/sql          backup/autodump     recuperación transportable
localStorage     frontend            preferencias puramente visuales
```

No hay transacción global entre filesystem, Docker y DB. Los flujos ordenan pasos y usan operaciones idempotentes/best-effort donde procede; un fallo intermedio puede requerir reintento o limpieza.

## Deuda observable

Algunas escrituras usan `std::fs::write` directo sin rename atómico. Las credenciales locales de servicios son constantes conocidas. La reconstrucción de proyectos sin sidecar es deliberadamente aproximada. El detalle exhaustivo de formatos debe residir en `../referencia/*`.

## Fuentes primarias

- `src-tauri/src/config.rs::SiteConfig`, `Endpoint`, `load_all_sites`, `list_disconnected_sites`
- `src-tauri/src/groups.rs::list`, `create`
- `src-tauri/src/docker.rs::db_data_dir`, `migrate_db_to_volume`
- `src-tauri/src/autodump.rs::persist`
- `src-tauri/src/dumplog.rs::append`
- `src/lib/types.ts::SiteConfig`, `Endpoint`
