#!/usr/bin/env node
// Servidor MCP de Panel WP — envoltorio fino sobre `wordpress-panel-cli`.
//
// No reimplementa nada: cada herramienta lanza el CLI (que habla con el panel
// EN EJECUCIÓN por D-Bus) y devuelve su salida. Para los comandos que el CLI
// autodetecta por directorio, resolvemos el proyecto (por id o nombre) a su
// carpeta y lanzamos el CLI con esa carpeta como cwd.
//
// Protocolo MCP por stdio: JSON-RPC 2.0, un mensaje por línea. Solo se necesita
// initialize / tools/list / tools/call, así que se implementa a mano (sin deps).
// IMPORTANTE: stdout es SOLO protocolo; los logs van a stderr.

import { spawn } from 'node:child_process';
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createInterface } from 'node:readline';

const HOME = homedir();
const PANEL_ROOT = process.env.PANEL_WP_ROOT || join(HOME, 'panel-wp');
const REPO_ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
// CLI: env explícito → wrapper instalado → script del repo.
const CLI =
  process.env.WORDPRESS_PANEL_CLI ||
  [join(HOME, '.local/bin/wordpress-panel-cli'), join(REPO_ROOT, 'scripts/wordpress-panel-cli.sh')].find(existsSync) ||
  'wordpress-panel-cli';

const log = (...a) => process.stderr.write('[wp-mcp] ' + a.join(' ') + '\n');

// ── Resolución de proyectos (espejo de project_for/resolve_pid del CLI) ──────
function loadProjects() {
  let dirs = [];
  try {
    dirs = readdirSync(PANEL_ROOT, { withFileTypes: true }).filter((d) => d.isDirectory());
  } catch {
    return [];
  }
  const out = [];
  for (const d of dirs) {
    const cfg = join(PANEL_ROOT, d.name, 'config.json');
    try {
      const c = JSON.parse(readFileSync(cfg, 'utf8'));
      if (c.id && c.path) out.push({ id: c.id, name: c.name || d.name, path: c.path, domain: c.domain, group: c.group });
    } catch {
      /* carpeta sin config válido → se ignora */
    }
  }
  return out;
}

// arg → proyecto por id exacto o nombre (subcadena, ci). Lanza si 0 o ambiguo.
function resolveProject(arg) {
  if (!arg) throw new Error('falta el parámetro «project» (id o nombre)');
  const projects = loadProjects();
  const byId = projects.find((p) => p.id === arg);
  if (byId) return byId;
  const a = arg.toLowerCase();
  const matches = projects.filter((p) => p.name.toLowerCase().includes(a));
  if (matches.length === 0) throw new Error(`no hay proyecto que coincida con «${arg}»`);
  if (matches.length > 1) throw new Error(`«${arg}» es ambiguo: ${matches.map((m) => m.name).join(', ')}`);
  return matches[0];
}

// ── Ejecutar el CLI ──────────────────────────────────────────────────────────
function runCli(args, cwd) {
  return new Promise((resolve) => {
    const env = { ...process.env, PATH: `${join(HOME, '.local/bin')}:${process.env.PATH || ''}` };
    const child = spawn(CLI, args, { cwd: cwd || REPO_ROOT, env });
    let out = '';
    let err = '';
    child.stdout.on('data', (d) => (out += d));
    child.stderr.on('data', (d) => (err += d));
    child.on('error', (e) => resolve({ code: 127, out, err: (err + '\n' + e.message).trim() }));
    child.on('close', (code) => resolve({ code, out, err }));
  });
}

// ── Catálogo de herramientas ─────────────────────────────────────────────────
// Cada una: nombre, descripción, schema de entrada, y build(args)→{argv, needProject}.
const S = {
  project: { type: 'string', description: 'id o nombre (subcadena) del proyecto' },
};
const req = (...names) => names;

const TOOLS = [
  {
    name: 'list_projects',
    description: 'Lista TODOS los proyectos WordPress con su estado (activo/parado), dominio y grupo.',
    schema: {},
    build: () => ({ argv: ['list'] }),
  },
  {
    name: 'start_project',
    description: 'Enciende un proyecto (containers + nginx + auto-dump).',
    schema: { project: S.project },
    required: req('project'),
    build: (a) => ({ argv: ['start', a.project] }),
  },
  {
    name: 'stop_project',
    description: 'Apaga un proyecto y los servicios compartidos que ya nadie use.',
    schema: { project: S.project },
    required: req('project'),
    build: (a) => ({ argv: ['stop', a.project] }),
  },
  {
    name: 'project_containers',
    description: 'Lista los containers de un proyecto (php, db, nginx, mailpit, minio) y su estado.',
    schema: { project: S.project },
    required: req('project'),
    build: () => ({ argv: ['containers'], needProject: true }),
  },
  {
    name: 'project_resources',
    description: 'Uso de CPU/memoria (docker stats) de los containers del proyecto.',
    schema: { project: S.project },
    required: req('project'),
    build: () => ({ argv: ['resources'], needProject: true }),
  },
  {
    name: 'project_logs',
    description: 'Logs de un container del proyecto. servicio ∈ php|db|nginx|mailpit|minio (default php).',
    schema: {
      project: S.project,
      service: { type: 'string', enum: ['php', 'db', 'nginx', 'mailpit', 'minio'], description: 'container (default php)' },
      lines: { type: 'integer', description: 'nº de líneas a mostrar (default 200)' },
    },
    required: req('project'),
    build: (a) => ({ argv: ['logs', a.service || 'php', '-n', String(a.lines || 200)], needProject: true }),
  },
  {
    name: 'set_php_upload_limit',
    description:
      'Ajusta el tope de subida del proyecto (upload_max_filesize + post_max_size) para evitar el 413 al subir themes/plugins grandes. Se aplica en caliente si el proyecto está activo. mb=0 vuelve al default del panel (64M).',
    schema: {
      project: S.project,
      mb: { type: 'integer', description: 'tope en MB (0 = default del panel, 64M)' },
    },
    required: req('project', 'mb'),
    build: (a) => ({ argv: ['php', 'upload', String(a.mb)], needProject: true }),
  },
  {
    name: 'open_project',
    description: 'Abre en el escritorio el wp-admin (auto-login), el frontend o la carpeta del proyecto.',
    schema: {
      project: S.project,
      what: { type: 'string', enum: ['admin', 'site', 'folder'], description: 'qué abrir' },
    },
    required: req('project', 'what'),
    build: (a) => ({ argv: ['open', a.what], needProject: true }),
  },
  {
    name: 'admin_login_url',
    description:
      'Devuelve una URL de auto-login del proyecto para abrirla en CUALQUIER navegador (revisiones que exigen sesión iniciada). Token de un solo uso, válido 300 s: pide una nueva por cada carga. Sin «user» entra como el primer administrador.',
    schema: {
      project: S.project,
      user: { type: 'string', description: 'ID numérico o user_login de WordPress (default: primer admin)' },
    },
    required: req('project'),
    build: (a) => ({ argv: ['login-url', ...(a.user ? ['--user', String(a.user)] : [])], needProject: true }),
  },
  {
    name: 'list_snapshots',
    description: 'Lista los puntos de guardado (snapshots) de un proyecto.',
    schema: { project: S.project },
    required: req('project'),
    build: () => ({ argv: ['snapshot', 'list'], needProject: true }),
  },
  {
    name: 'create_snapshot',
    description: 'Crea un punto de guardado (código + dump SQL) del proyecto.',
    schema: { project: S.project, label: { type: 'string', description: 'etiqueta del snapshot' } },
    required: req('project', 'label'),
    build: (a) => ({ argv: ['snapshot', 'create', a.label], needProject: true }),
  },
  {
    name: 'delete_snapshot',
    description: 'Borra un punto de guardado del disco.',
    schema: { project: S.project, snapshotId: { type: 'string' } },
    required: req('project', 'snapshotId'),
    build: (a) => ({ argv: ['snapshot', 'delete', a.snapshotId], needProject: true }),
  },
  {
    name: 'clone_snapshot',
    description: 'Levanta un clon temporal a partir de un punto de guardado.',
    schema: { project: S.project, snapshotId: { type: 'string' } },
    required: req('project', 'snapshotId'),
    build: (a) => ({ argv: ['snapshot', 'clone', a.snapshotId], needProject: true }),
  },
  {
    name: 'git_scan',
    description: 'Lista los repos git de un proyecto (rama, remoto, si está registrado para deploy).',
    schema: { project: S.project },
    required: req('project'),
    build: () => ({ argv: ['git', 'scan'], needProject: true }),
  },
  {
    name: 'git_status',
    description: 'Estado de una rama vs remoto (fetch + ahead/behind + árbol sucio + canPull).',
    schema: {
      project: S.project,
      path: { type: 'string', description: 'ruta del repo relativa a app/public/' },
      branch: { type: 'string', description: 'rama a comparar (default la actual)' },
    },
    required: req('project', 'path'),
    build: (a) => ({ argv: ['git', 'status', '--path', a.path, ...(a.branch ? ['--branch', a.branch] : [])], needProject: true }),
  },
  {
    name: 'git_pull',
    description: 'git pull de una rama de un repo del proyecto.',
    schema: {
      project: S.project,
      path: { type: 'string', description: 'ruta del repo relativa a app/public/' },
      branch: { type: 'string' },
    },
    required: req('project', 'path'),
    build: (a) => ({ argv: ['git', 'pull', '--path', a.path, ...(a.branch ? ['--branch', a.branch] : [])], needProject: true }),
  },
  {
    name: 'git_set_deploy',
    description: 'Configura el deploy directo de un repo: rama, comando de build y carpetas de build.',
    schema: {
      project: S.project,
      path: { type: 'string', description: 'ruta del repo relativa a app/public/' },
      branch: { type: 'string' },
      build: { type: 'string', description: 'comando de build, p.ej. "npm ci && npm run build"' },
      dirs: { type: 'string', description: 'carpetas de build separadas por coma, p.ej. "dist" o "src,src-redesign"' },
    },
    required: req('project', 'path', 'branch'),
    build: (a) => ({
      argv: ['git', 'set-deploy', '--path', a.path, '--branch', a.branch, ...(a.build ? ['--build', a.build] : []), ...(a.dirs ? ['--dirs', a.dirs] : [])],
      needProject: true,
    }),
  },
  {
    name: 'git_deploy',
    description: 'Ejecuta el deploy guardado de un repo (checkout + git pull --ff-only + build).',
    schema: { project: S.project, path: { type: 'string', description: 'ruta del repo relativa a app/public/' } },
    required: req('project', 'path'),
    build: (a) => ({ argv: ['git', 'deploy', '--path', a.path], needProject: true }),
  },
  {
    name: 'worktree_list',
    description: 'Lista los worktree-projects (ramas de theme/plugin en aislamiento) de un proyecto.',
    schema: { project: S.project },
    required: req('project'),
    build: () => ({ argv: ['worktree', 'list'], needProject: true }),
  },
  {
    name: 'worktree_create',
    description: 'Crea un worktree-project: prueba una rama de un theme/plugin en aislamiento (sin duplicar WordPress).',
    schema: {
      project: S.project,
      branch: { type: 'string', description: 'rama nueva del worktree' },
      target: { type: 'string', description: 'ruta del repo relativa a app/public/ (default: repo del cwd)' },
      base: { type: 'string', description: 'rama base de la que partir (default: la actual)' },
      copyDb: { type: 'boolean', description: 'copiar la BD en vez de compartirla (default false = compartida)' },
    },
    required: req('project', 'branch'),
    build: (a) => ({
      argv: ['worktree', 'create', a.branch, ...(a.target ? ['--target', a.target] : []), ...(a.base ? ['--base', a.base] : []), ...(a.copyDb ? ['--copy-db'] : [])],
      needProject: true,
    }),
  },
  {
    name: 'worktree_remove',
    description: 'Elimina un worktree-project (git worktree remove + borra carpeta). La rama queda salvo --delete-branch.',
    schema: {
      project: S.project,
      worktreeId: { type: 'string', description: 'id del worktree (de worktree_list)' },
      deleteBranch: { type: 'boolean', description: 'borrar también la rama git (default false)' },
    },
    required: req('project', 'worktreeId'),
    build: (a) => ({
      argv: ['worktree', 'remove', a.worktreeId, ...(a.deleteBranch ? ['--delete-branch'] : [])],
      needProject: true,
    }),
  },
];

// ── Manejadores JSON-RPC ──────────────────────────────────────────────────────
const PROTOCOL = '2024-11-05';

function toolsList() {
  return {
    tools: TOOLS.map((t) => ({
      name: t.name,
      description: t.description,
      inputSchema: { type: 'object', properties: t.schema, required: t.required || [] },
    })),
  };
}

async function toolsCall(params) {
  const tool = TOOLS.find((t) => t.name === params?.name);
  if (!tool) return { content: [{ type: 'text', text: `herramienta desconocida: ${params?.name}` }], isError: true };
  const args = params.arguments || {};
  try {
    const { argv, needProject } = tool.build(args);
    let cwd;
    if (needProject) cwd = resolveProject(args.project).path;
    const { code, out, err } = await runCli(argv, cwd);
    const text = [out.trim(), err.trim()].filter(Boolean).join('\n') || '(sin salida)';
    return { content: [{ type: 'text', text }], isError: code !== 0 };
  } catch (e) {
    return { content: [{ type: 'text', text: String(e.message || e) }], isError: true };
  }
}

async function handle(msg) {
  const { id, method, params } = msg;
  const reply = (result) => ({ jsonrpc: '2.0', id, result });
  const fail = (code, message) => ({ jsonrpc: '2.0', id, error: { code, message } });
  switch (method) {
    case 'initialize':
      return reply({
        protocolVersion: params?.protocolVersion || PROTOCOL,
        capabilities: { tools: {} },
        serverInfo: { name: 'wordpress-panel', version: '0.1.0' },
      });
    case 'ping':
      return reply({});
    case 'tools/list':
      return reply(toolsList());
    case 'tools/call':
      return reply(await toolsCall(params));
    case 'notifications/initialized':
    case 'notifications/cancelled':
      return null; // notificación: sin respuesta
    default:
      if (id === undefined) return null; // otra notificación desconocida
      return fail(-32601, `método no soportado: ${method}`);
  }
}

// ── Bucle stdio ────────────────────────────────────────────────────────────────
const rl = createInterface({ input: process.stdin });
rl.on('line', async (line) => {
  const s = line.trim();
  if (!s) return;
  let msg;
  try {
    msg = JSON.parse(s);
  } catch {
    log('línea no-JSON ignorada');
    return;
  }
  try {
    const res = await handle(msg);
    if (res) process.stdout.write(JSON.stringify(res) + '\n');
  } catch (e) {
    log('error manejando', msg?.method, String(e));
    if (msg?.id !== undefined) process.stdout.write(JSON.stringify({ jsonrpc: '2.0', id: msg.id, error: { code: -32603, message: String(e) } }) + '\n');
  }
});
log(`listo — CLI=${CLI} PANEL_WP_ROOT=${PANEL_ROOT}`);
