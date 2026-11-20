// Legacy-format conversion sidecar: wraps @deepseek-ai/libreoffice-kit as a
// minimal local HTTP service. The Rust backend spawns this with node.exe,
// reads "PORT=<n>" from stdout, and POSTs raw bytes to /convert.
// Endpoints:
//   GET  /health              -> 200 ok
//   POST /convert?from=a&to=b -> body = source bytes, response = converted bytes
// Errors return non-200 with a plain-text message.
import http from 'node:http';
import os from 'node:os';
import path from 'node:path';
import fs from 'node:fs';
import { createConverter } from '@deepseek-ai/libreoffice-kit';

const WORK = path.join(os.tmpdir(), 'convert-svc');
fs.mkdirSync(WORK, { recursive: true });

// Parent liveness: the Rust backend holds our stdin open; when it dies the
// pipe closes and we exit instead of lingering as an orphan.
process.stdin.resume();
process.stdin.on('end', () => process.exit(0));
process.stdin.on('error', () => process.exit(0));

const converter = await createConverter();

let seq = 0;
async function convertBytes(from, to, bytes) {
  const n = ++seq;
  const src = path.join(WORK, `in-${n}.${from}`);
  const dest = path.join(WORK, `out-${n}.${to}`);
  try {
    fs.writeFileSync(src, bytes);
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 110_000);
    try {
      await converter.convert({ inputPath: src, outputPath: dest }, controller.signal);
    } finally {
      clearTimeout(timer);
    }
    return fs.readFileSync(dest);
  } finally {
    fs.rmSync(src, { force: true });
    fs.rmSync(dest, { force: true });
  }
}

const server = http.createServer((req, res) => {
  if (req.method === 'GET' && req.url === '/health') {
    res.writeHead(200).end('ok');
    return;
  }
  const url = new URL(req.url, 'http://127.0.0.1');
  if (req.method !== 'POST' || url.pathname !== '/convert') {
    res.writeHead(404).end('not found');
    return;
  }
  const from = url.searchParams.get('from') || '';
  const to = url.searchParams.get('to') || '';
  if (!/^[a-z0-9]{1,8}$/i.test(from) || !/^[a-z0-9]{1,8}$/i.test(to)) {
    res.writeHead(400).end('bad from/to');
    return;
  }
  const chunks = [];
  let size = 0;
  req.on('data', (c) => {
    size += c.length;
    if (size > 64 * 1024 * 1024) { req.destroy(); return; }
    chunks.push(c);
  });
  req.on('end', async () => {
    try {
      const out = await convertBytes(from, to, Buffer.concat(chunks));
      res.writeHead(200, { 'Content-Type': 'application/octet-stream' });
      res.end(out);
    } catch (err) {
      const msg = err && err.message ? String(err.message) : 'convert failed';
      res.writeHead(500, { 'Content-Type': 'text/plain; charset=utf-8' });
      res.end(msg);
    }
  });
});

server.listen(0, '127.0.0.1', () => {
  console.log(`PORT=${server.address().port}`);
});
