import { spawn } from 'node:child_process';
import { mkdtempSync, existsSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const PORT = Number(process.env.PAPERCRAFT_E2E_PORT || 29982);
const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../../..');
// The gate builds every worktree into the shared target folder.
const BIN = process.env.RELAY_BIN || 'D:/Projects/papercraft-wt/target/release/papercraft-relay.exe';
export const WEB_DIR = process.env.WEB_DIR || path.join(repo, 'web');

/** Starts the relay on its own port and data dir. Only this PID is ever stopped. */
export async function startRelay(extraArgs = []) {
  if (!existsSync(BIN)) throw new Error(`relay binary missing: ${BIN} (build it through the gate, see README)`);
  if (!existsSync(path.join(WEB_DIR, 'pkg', 'papercraft.js'))) throw new Error(`web build missing in ${WEB_DIR}/pkg (run web/build.sh)`);
  const data = mkdtempSync(path.join(tmpdir(), 'pc-e2e-'));
  const child = spawn(BIN, ['--listen', `127.0.0.1:${PORT}`, '--static', WEB_DIR, '--data', data, ...extraArgs], { stdio: ['ignore', 'pipe', 'pipe'] });
  let log = '';
  child.stdout.on('data', (d) => (log += d));
  child.stderr.on('data', (d) => (log += d));
  const exited = new Promise((r) => child.once('exit', r));
  for (let i = 0; i < 100; i++) {
    if (child.exitCode !== null) throw new Error(`relay exited early: ${log}`);
    if (/listening on port/.test(log)) break;
    await new Promise((r) => setTimeout(r, 100));
  }
  if (!/listening on port/.test(log)) throw new Error(`relay did not start: ${log}`);
  return {
    origin: `http://127.0.0.1:${PORT}`,
    pid: child.pid,
    async stop() {
      child.kill();
      await Promise.race([exited, new Promise((r) => setTimeout(r, 5000))]);
      rmSync(data, { recursive: true, force: true });
    },
  };
}
