import https from 'node:https';

const version = process.argv[2];
const sha256 = process.argv[3];
const token = process.env.HOMEBREW_TAP_TOKEN || process.env.GH_TOKEN;

if (!version || !sha256) {
  console.error('Usage: node scripts/update-homebrew-tap.js <version> <sha256>');
  process.exit(1);
}

const cleanVersion = version.replace(/^v/, '');
const isPrerelease = cleanVersion.includes('-');
const path = isPrerelease ? 'Casks/limen@beta.rb' : 'Casks/limen.rb';
const caskName = isPrerelease ? 'limen@beta' : 'limen';

if (!token) {
  console.warn('⚠️  HOMEBREW_TAP_TOKEN secret is not set. Skipping Homebrew tap update.');
  console.warn('To enable auto-updates, add a GitHub PAT with repo scope as secret "HOMEBREW_TAP_TOKEN".');
  process.exit(0);
}

const repo = 'ranajoy-dutta/homebrew-tap';

function githubRequest(method, endpoint, body = null) {
  return new Promise((resolve, reject) => {
    const dataString = body ? JSON.stringify(body) : null;
    const req = https.request(
      {
        hostname: 'api.github.com',
        path: endpoint,
        method,
        headers: {
          Authorization: `Bearer ${token}`,
          Accept: 'application/vnd.github.v3+json',
          'User-Agent': 'limen-release-action',
          ...(dataString
            ? {
                'Content-Type': 'application/json',
                'Content-Length': Buffer.byteLength(dataString),
              }
            : {}),
        },
      },
      (res) => {
        let resBody = '';
        res.on('data', (chunk) => {
          resBody += chunk;
        });
        res.on('end', () => {
          if (res.statusCode >= 200 && res.statusCode < 300) {
            try {
              resolve(JSON.parse(resBody));
            } catch {
              resolve(resBody);
            }
          } else {
            reject(
              new Error(`GitHub API error (${res.statusCode}): ${resBody}`)
            );
          }
        });
      }
    );

    req.on('error', reject);
    if (dataString) {
      req.write(dataString);
    }
    req.end();
  });
}

async function run() {
  console.log(`Targeting ${path} in ${repo} (${isPrerelease ? 'Pre-release / Beta' : 'Stable'})...`);

  let currentContent;
  let fileSha;

  try {
    const fileData = await githubRequest('GET', `/repos/${repo}/contents/${path}`);
    currentContent = Buffer.from(fileData.content, 'base64').toString('utf8');
    fileSha = fileData.sha;
  } catch (err) {
    if (isPrerelease && err.message.includes('404')) {
      console.log(`${path} does not exist yet. Initializing from Casks/limen.rb...`);
      const baseFileData = await githubRequest('GET', `/repos/${repo}/contents/Casks/limen.rb`);
      const baseContent = Buffer.from(baseFileData.content, 'base64').toString('utf8');
      currentContent = baseContent
        .replace(/cask "limen"/, 'cask "limen@beta"')
        .replace(/name "Limen"/, 'name "Limen (Beta)"')
        .replace(/desc "([^"]+)"/, 'desc "$1 (Pre-release)"');
    } else {
      throw err;
    }
  }

  const updatedContent = currentContent
    .replace(/version\s+"[^"]+"/, `version "${cleanVersion}"`)
    .replace(/sha256\s+"[^"]+"/, `sha256 "${sha256}"`);

  if (fileSha && updatedContent === currentContent) {
    console.log(`Cask is already up to date with version ${cleanVersion}.`);
    return;
  }

  console.log(`Updating ${path} to version ${cleanVersion} with sha256 ${sha256}...`);
  await githubRequest('PUT', `/repos/${repo}/contents/${path}`, {
    message: `bump: ${caskName} cask to v${cleanVersion}`,
    content: Buffer.from(updatedContent).toString('base64'),
    ...(fileSha ? { sha: fileSha } : {}),
  });

  console.log(`✅ Successfully updated Homebrew tap (${path}) to v${cleanVersion}!`);
}

run().catch((err) => {
  console.error('Error updating Homebrew tap:', err);
  process.exit(1);
});
