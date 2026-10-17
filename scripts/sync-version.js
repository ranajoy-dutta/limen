import fs from 'node:fs';

const version = process.argv[2];
if (!version) {
  console.error('Usage: node scripts/sync-version.js <version>');
  process.exit(1);
}

const cleanVersion = version.replace(/^v/, '');
console.log(`Synchronizing project version to ${cleanVersion}...`);

// 1. Update package.json
const pkgPath = 'package.json';
const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf8'));
pkg.version = cleanVersion;
fs.writeFileSync(pkgPath, JSON.stringify(pkg, null, 2) + '\n');
console.log(`Updated ${pkgPath}`);

// 2. Update package-lock.json if present
const lockPath = 'package-lock.json';
if (fs.existsSync(lockPath)) {
  const lock = JSON.parse(fs.readFileSync(lockPath, 'utf8'));
  lock.version = cleanVersion;
  if (lock.packages && lock.packages['']) {
    lock.packages[''].version = cleanVersion;
  }
  fs.writeFileSync(lockPath, JSON.stringify(lock, null, 2) + '\n');
  console.log(`Updated ${lockPath}`);
}

// 3. Update src-tauri/tauri.conf.json
const tauriPath = 'src-tauri/tauri.conf.json';
if (fs.existsSync(tauriPath)) {
  const tauri = JSON.parse(fs.readFileSync(tauriPath, 'utf8'));
  tauri.version = cleanVersion;
  fs.writeFileSync(tauriPath, JSON.stringify(tauri, null, 2) + '\n');
  console.log(`Updated ${tauriPath}`);
}

// 4. Update src-tauri/Cargo.toml
const cargoPath = 'src-tauri/Cargo.toml';
if (fs.existsSync(cargoPath)) {
  let cargo = fs.readFileSync(cargoPath, 'utf8');
  cargo = cargo.replace(/^version\s*=\s*"[^"]+"/m, `version = "${cleanVersion}"`);
  fs.writeFileSync(cargoPath, cargo);
  console.log(`Updated ${cargoPath}`);
}

// 5. Update src-tauri/Cargo.lock if present
const cargoLockPath = 'src-tauri/Cargo.lock';
if (fs.existsSync(cargoLockPath)) {
  let cargoLock = fs.readFileSync(cargoLockPath, 'utf8');
  cargoLock = cargoLock.replace(
    /(\[\[package\]\]\r?\nname\s*=\s*"limen"\r?\nversion\s*=\s*")[^"]+(")/,
    `$1${cleanVersion}$2`
  );
  fs.writeFileSync(cargoLockPath, cargoLock);
  console.log(`Updated ${cargoLockPath}`);
}

// 6. Update Casks/limen.rb if present
const caskPath = 'Casks/limen.rb';
if (fs.existsSync(caskPath)) {
  let cask = fs.readFileSync(caskPath, 'utf8');
  cask = cask.replace(/version\s+"[^"]+"/, `version "${cleanVersion}"`);
  fs.writeFileSync(caskPath, cask);
  console.log(`Updated ${caskPath}`);
}

console.log('Version synchronization complete.');
