import { spawnSync } from 'node:child_process';

// npm supplies its CLI path on both Windows and macOS; avoid shell-specific chaining.
const npm = process.env.npm_execpath;
if (!npm) throw new Error('Run this script with npm run check');
const steps = [
  ['Frontend tests', process.execPath, [npm, 'test']],
  ['Frontend build', process.execPath, [npm, 'run', 'build']],
  ...(!process.env.TP_TEST_BROWSER ? [
    ['Chromium setup', process.execPath, [npm, 'exec', '--', 'playwright', 'install', '--only-shell', 'chromium']],
  ] : []),
  ['Browser UI tests', process.execPath, [npm, 'run', 'test:ui']],
  ['Rust check', 'cargo', ['check', '--locked', '--manifest-path', 'src-tauri/Cargo.toml']],
  ['Rust tests', 'cargo', ['test', '--locked', '--manifest-path', 'src-tauri/Cargo.toml', '--lib']],
];
for (const [label, executable, args] of steps) {
  console.log(`\n== ${label} ==`);
  const result = spawnSync(executable, args, { stdio: 'inherit' });
  if (result.error || result.status !== 0) {
    console.error(`${label} failed: ${result.error?.message ?? result.signal ?? result.status}`);
    process.exit(result.status || 1);
  }
}
console.log('\nAll local checks passed.');
