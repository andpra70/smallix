#!/usr/bin/env node
import fs from "node:fs";
import fsp from "node:fs/promises";
import http from "node:http";
import path from "node:path";
import process from "node:process";

const BLOCK_SECTORS = 256;
const SECTOR_SIZE = 512;
const BLOCK_BYTES = BLOCK_SECTORS * SECTOR_SIZE;

const BLK_TXT_PATHS = [
  "/www/tinyemu-disks/smallix/blk.txt",
  "/tinyemu-disks/smallix/blk.txt",
];
const BLK_BIN_RE = /^\/(?:www\/)?tinyemu-disks\/smallix\/blk(\d{9})\.bin(?:\.en)?$/i;
const GRP_BIN_RE = /^\/(?:www\/)?tinyemu-disks\/smallix\/grp(\d{9})\.bin$/i;
const PREFETCH_GROUP_LEN = 1;
const PREFETCH_MAX = 16;

function parseArgs(argv) {
  const out = { root: "", disk: "", port: 8000 };
  for (let i = 2; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--root") {
      out.root = argv[++i] ?? "";
    } else if (a === "--disk") {
      out.disk = argv[++i] ?? "";
    } else if (a === "--port") {
      out.port = Number(argv[++i] ?? "8000");
    } else if (a === "-h" || a === "--help") {
      usage(0);
    } else {
      console.error(`Unknown argument: ${a}`);
      usage(1);
    }
  }
  if (!out.root || !out.disk || !Number.isInteger(out.port) || out.port <= 0) {
    usage(1);
  }
  return out;
}

function usage(exitCode) {
  console.error("Usage: tinyemu-single-disk-server.mjs --root <path> --disk <img|iso> [--port <n>]");
  process.exit(exitCode);
}

function prefetchList(nBlocks) {
  return Array.from({ length: Math.min(nBlocks, PREFETCH_MAX) }, (_, i) => i);
}

function mimeType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  switch (ext) {
    case ".html":
      return "text/html; charset=utf-8";
    case ".js":
    case ".mjs":
      return "application/javascript; charset=utf-8";
    case ".css":
      return "text/css; charset=utf-8";
    case ".json":
      return "application/json; charset=utf-8";
    case ".wasm":
      return "application/wasm";
    case ".png":
      return "image/png";
    case ".svg":
      return "image/svg+xml";
    case ".ico":
      return "image/x-icon";
    case ".txt":
    case ".cfg":
      return "text/plain; charset=utf-8";
    case ".bin":
    case ".img":
    case ".iso":
      return "application/octet-stream";
    default:
      return "application/octet-stream";
  }
}

function sendError(res, code, message) {
  const body = `${message}\n`;
  res.writeHead(code, {
    "Content-Type": "text/plain; charset=utf-8",
    "Content-Length": Buffer.byteLength(body),
  });
  res.end(body);
}

async function main() {
  const args = parseArgs(process.argv);
  const root = path.resolve(args.root);
  const disk = path.isAbsolute(args.disk) ? args.disk : path.resolve(root, args.disk);

  let diskStat;
  try {
    const st = await fsp.stat(disk);
    if (!st.isFile()) throw new Error("not file");
    diskStat = st;
  } catch {
    console.error(`Disk image not found or invalid: ${disk}`);
    process.exit(1);
  }

  let rootStat;
  try {
    const st = await fsp.stat(root);
    if (!st.isDirectory()) throw new Error("not dir");
    rootStat = st;
  } catch {
    console.error(`Root path not found or invalid: ${root}`);
    process.exit(1);
  }

  void rootStat;

  const server = http.createServer(async (req, res) => {
    try {
      if (!req.url) {
        sendError(res, 400, "Bad request");
        return;
      }
      const method = req.method ?? "GET";
      if (method !== "GET" && method !== "HEAD") {
        sendError(res, 405, "Method not allowed");
        return;
      }

      const rawPath = decodeURIComponent(req.url.split("?")[0] || "/");

      if (BLK_TXT_PATHS.includes(rawPath)) {
        const nBlocks = Math.ceil(diskStat.size / BLOCK_BYTES);
        const payload = JSON.stringify({
          block_size: BLOCK_SECTORS,
          n_block: nBlocks,
          prefetch: prefetchList(nBlocks),
          prefetch_group_len: PREFETCH_GROUP_LEN,
        });
        const length = Buffer.byteLength(payload);
        res.writeHead(200, {
          "Content-Type": "application/json; charset=utf-8",
          "Content-Length": length,
          "Cache-Control": "no-store, no-cache, must-revalidate, max-age=0",
          Pragma: "no-cache",
        });
        if (method === "HEAD") res.end();
        else res.end(payload);
        console.log(`[disk] ${method} ${rawPath} -> 200 json n_block=${nBlocks}`);
        return;
      }

      const m = rawPath.match(BLK_BIN_RE);
      if (m) {
        const blockIndex = Number(m[1]);
        const offset = blockIndex * BLOCK_BYTES;

        const buffer = Buffer.alloc(BLOCK_BYTES);
        const fd = await fsp.open(disk, "r");
        try {
          let totalRead = 0;
          while (totalRead < BLOCK_BYTES) {
            const { bytesRead } = await fd.read(
              buffer,
              totalRead,
              BLOCK_BYTES - totalRead,
              offset + totalRead
            );
            if (bytesRead === 0) break;
            totalRead += bytesRead;
          }
        } finally {
          await fd.close();
        }

        res.writeHead(200, {
          "Content-Type": "application/octet-stream",
          "Content-Length": BLOCK_BYTES,
          "Cache-Control": "no-store, no-cache, must-revalidate, max-age=0",
          Pragma: "no-cache",
        });
        if (method === "HEAD") res.end();
        else res.end(buffer);
        console.log(`[disk] ${method} ${rawPath} -> 200 bytes=${BLOCK_BYTES}`);
        return;
      }

      const gm = rawPath.match(GRP_BIN_RE);
      if (gm) {
        const groupIndex = Number(gm[1]);
        const nBlocks = Math.ceil(diskStat.size / BLOCK_BYTES);
        const pref = prefetchList(nBlocks);
        const start = groupIndex * PREFETCH_GROUP_LEN;
        const group = pref.slice(start, start + PREFETCH_GROUP_LEN);
        if (group.length === 0) {
          sendError(res, 404, "Group not found");
          return;
        }

        const out = Buffer.alloc(group.length * BLOCK_BYTES);
        const fd = await fsp.open(disk, "r");
        try {
          for (let i = 0; i < group.length; i++) {
            const blockIndex = group[i];
            const offset = blockIndex * BLOCK_BYTES;
            let totalRead = 0;
            while (totalRead < BLOCK_BYTES) {
              const { bytesRead } = await fd.read(
                out,
                i * BLOCK_BYTES + totalRead,
                BLOCK_BYTES - totalRead,
                offset + totalRead
              );
              if (bytesRead === 0) break;
              totalRead += bytesRead;
            }
          }
        } finally {
          await fd.close();
        }

        res.writeHead(200, {
          "Content-Type": "application/octet-stream",
          "Content-Length": out.length,
          "Cache-Control": "no-store, no-cache, must-revalidate, max-age=0",
          Pragma: "no-cache",
        });
        if (method === "HEAD") res.end();
        else res.end(out);
        console.log(`[disk] ${method} ${rawPath} -> 200 bytes=${out.length}`);
        return;
      }

      const rel = rawPath === "/" ? "/index.html" : rawPath;
      const fsPath = path.resolve(root, `.${rel}`);
      if (!fsPath.startsWith(root + path.sep) && fsPath !== root) {
        sendError(res, 403, "Forbidden");
        return;
      }

      let st;
      try {
        st = await fsp.stat(fsPath);
      } catch {
        sendError(res, 404, "Not found");
        return;
      }

      let filePath = fsPath;
      if (st.isDirectory()) {
        filePath = path.join(fsPath, "index.html");
      }

      let fileStat;
      try {
        fileStat = await fsp.stat(filePath);
        if (!fileStat.isFile()) throw new Error("not file");
      } catch {
        sendError(res, 404, "Not found");
        return;
      }

      res.writeHead(200, {
        "Content-Type": mimeType(filePath),
        "Content-Length": fileStat.size,
      });
      if (method === "HEAD") {
        res.end();
        return;
      }
      fs.createReadStream(filePath).pipe(res);
    } catch (err) {
      console.error("server error:", err);
      sendError(res, 500, "Internal server error");
    }
  });

  server.listen(args.port, "127.0.0.1", () => {
    const nBlocks = Math.ceil(diskStat.size / BLOCK_BYTES);
    console.log(`TinyEMU web server on http://127.0.0.1:${args.port}`);
    console.log(`Serving root: ${root}`);
    console.log(`Virtual disk source: ${disk}`);
    console.log(`Virtual disk size: ${diskStat.size} bytes`);
    console.log(`Virtual n_block: ${nBlocks} (block_size=${BLOCK_SECTORS} sectors, ${BLOCK_BYTES} bytes)`);
  });
}

await main();
