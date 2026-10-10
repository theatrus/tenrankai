import { execFileSync } from 'node:child_process';
import { copyFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));
const PHOTOS = join(HERE, 'fixtures', 'photos');
const CACHE = join(HERE, '.cache');

// CRC32 (PNG / zlib polynomial) — precomputed table.
const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    table[n] = c >>> 0;
  }
  return table;
})();

function crc32(buf) {
  let c = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  }
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const typeBuf = Buffer.from(type, 'ascii');
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])), 0);
  return Buffer.concat([len, typeBuf, data, crc]);
}

/** Encode a solid-color RGB PNG with no external dependencies. */
function solidPng(width, height, rgb) {
  const sig = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 2; // color type: truecolor RGB
  // 10..12 already zero: compression, filter, interlace

  const stride = width * 3;
  const raw = Buffer.alloc((stride + 1) * height);
  for (let y = 0; y < height; y++) {
    const rowStart = y * (stride + 1);
    raw[rowStart] = 0; // filter: none
    for (let x = 0; x < width; x++) {
      const p = rowStart + 1 + x * 3;
      raw[p] = rgb[0];
      raw[p + 1] = rgb[1];
      raw[p + 2] = rgb[2];
    }
  }

  return Buffer.concat([
    sig,
    chunk('IHDR', ihdr),
    chunk('IDAT', deflateSync(raw)),
    chunk('IEND', Buffer.alloc(0)),
  ]);
}

// Five images shared by every test folder. Distinct colors so failures are
// visually obvious in traces; filenames chosen so alphabetical order is clear.
const IMAGES = [
  { name: '01-alpha.png', rgb: [220, 60, 60] },
  { name: '02-bravo.png', rgb: [60, 180, 75] },
  { name: '03-charlie.png', rgb: [60, 100, 220] },
  { name: '04-delta.png', rgb: [240, 200, 40] },
  { name: '05-echo.png', rgb: [150, 70, 200] },
];

function writeFolderImages(folder) {
  const dir = join(PHOTOS, folder);
  mkdirSync(dir, { recursive: true });
  for (const img of IMAGES) {
    writeFileSync(join(dir, img.name), solidPng(200, 200, img.rgb));
  }
}

// The interactive test mutates manage/_folder.md (the sort API rewrites it), so
// it is generated fresh each run and gitignored — keeps the tree clean and the
// starting state deterministic regardless of a previous run or a retry.
const MANAGE_FOLDER_MD = `+++
title = "Manage (interactive)"
grid_mode = "square"
sort_order = "filename"
sort_direction = "asc"

[permissions]
public_role = "viewer"

[permissions.roles.viewer]
name = "viewer"
permissions = { can_view = true }

[permissions.roles.owner]
name = "owner"
permissions = { can_view = true, owner_access = true, can_manage_images = true }

[[permissions.user_roles]]
username = "e2euser"
roles = ["owner"]
+++

# Manage (interactive)
`;

// Generate everything the server reads at startup. This runs from the
// Playwright `webServer.command` *before* `cargo run`, so the fresh server's
// folder scan always sees the fixtures (globalSetup runs after the web server
// starts, which is too late — the folder cache is built at startup).
function prepareFixtures() {
  // Start from a clean cache so freshly generated fixtures are never served
  // from a stale resize cache, and so the user database (with any passkeys
  // registered by a previous run) starts empty.
  rmSync(CACHE, { recursive: true, force: true });
  mkdirSync(CACHE, { recursive: true });

  // The _folder.md files are committed; only the image bytes are generated.
  for (const folder of ['by-filename', 'by-filename-desc', 'custom', 'display', 'manage']) {
    writeFolderImages(folder);
  }
  // ...except manage/, whose config the sort API rewrites — regenerate it.
  writeFileSync(join(PHOTOS, 'manage', '_folder.md'), MANAGE_FOLDER_MD);

  // Isolated mixed-media gallery: actual originals exercise automatic video
  // preparation and browser playback through the same server as photo tests.
  const videoRoot = join(HERE, 'fixtures', 'video-photos');
  rmSync(videoRoot, { recursive: true, force: true });
  const permissions = `[permissions]\npublic_role = "viewer"\n[permissions.roles.viewer]\nname = "viewer"\npermissions = { can_view = true }\n[permissions.roles.owner]\nname = "owner"\npermissions = { can_view = true, owner_access = true, can_manage_images = true, can_download_original = true, can_read_metadata = true, can_edit_content = true, can_set_picks = true, can_add_comments = true, can_see_technical_details = true, can_see_exact_dates = true, can_see_location = true }\n[[permissions.user_roles]]\nusername = "e2euser"\nroles = ["owner"]\n`;
  for (const folder of ['clips', 'private', 'uploads']) {
    mkdirSync(join(videoRoot, folder), { recursive: true });
    const folderPermissions = folder === 'private' ? permissions.replace('public_role = "viewer"', 'public_role = "none"') : permissions;
    const hidden = folder === 'clips' ? 'hidden_images = ["hidden.mov"]\n' : '';
    writeFileSync(join(videoRoot, folder, '_folder.md'), `+++\ntitle = "${folder}"\n${hidden}${folderPermissions}+++\n`);
  }
  writeFileSync(join(videoRoot, 'clips', 'scene.png'), solidPng(240, 160, [60, 100, 220]));
  const ffmpeg = process.env.TENRANKAI_FFMPEG || 'ffmpeg';
  const common = ['-nostdin', '-hide_banner', '-loglevel', 'error', '-y', '-filter_threads', '1'];
  execFileSync(ffmpeg, [...common, '-f', 'lavfi', '-i', 'testsrc2=size=240x160:rate=24',
    '-f', 'lavfi', '-i', 'sine=frequency=440:sample_rate=48000', '-t', '4',
    '-c:v', 'mpeg4', '-c:a', 'pcm_s16le', '-threads', '1',
    '-metadata', 'creation_time=2026-09-19T12:30:00Z',
    '-metadata', 'location=+37.7749-122.4194/', join(videoRoot, 'clips', 'scene.mov')]);
  execFileSync(ffmpeg, [...common, '-f', 'lavfi', '-i', 'testsrc2=size=96x160:rate=24',
    '-t', '2', '-c:v', 'mpeg4', '-threads', '1', join(videoRoot, 'clips', 'portrait.mkv')]);
  copyFileSync(join(videoRoot, 'clips', 'scene.mov'), join(videoRoot, 'clips', 'hidden.mov'));
  copyFileSync(join(videoRoot, 'clips', 'scene.mov'), join(videoRoot, 'private', 'secret.mov'));
  writeFileSync(join(videoRoot, 'clips', 'broken.mov'), 'This is not a video file.');

  // Seed the admin user with no passkeys; the passkey/interactive specs register
  // one through the UI. The TOML backend reloads this on change.
  writeFileSync(
    join(CACHE, 'users.toml'),
    '[users.e2euser]\nemail = "e2euser@example.com"\n',
  );
}

prepareFixtures();
