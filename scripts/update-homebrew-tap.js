import https from 'node:https';

const version = process.argv[2];
const sha256 = process.argv[3];
const token = process.env.HOMEBREW_TAP_TOKEN || process.env.GH_TOKEN;

if (!version || !sha256) {
  console.error('Usage: node scripts/update-homebrew-tap.js <version> <sha256>');
  process.exit(1);
}

const cleanVersion = version.replace(/^v/, '');

if (cleanVersion.includes('-')) {
  console.log(`Prerelease version detected (${cleanVersion}). Skipping Homebrew tap update.`);
  process.exit(0);
}

if (!token) {
  console.warn('⚠️  HOMEBREW_TAP_TOKEN secret is not set. Skipping Homebrew tap update.');
  console.warn('To enable auto-updates, add a GitHub PAT with repo scope as secret "HOMEBREW_TAP_TOKEN".');
  process.exit(0);
}

const repo = 'ranajoy-dutta/homebrew-tap';
const path = 'Casks/limen.rb';

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
  console.log(`Fetching current ${path} from ${repo}...`);
  const fileData = await githubRequest('GET', `/repos/${repo}/contents/${path}`);

  const currentContent = Buffer.from(fileData.content, 'base64').toString('utf8');

  const updatedContent = currentContent
    .replace(/version\s+"[^"]+"/, `version "${cleanVersion}"`)
    .replace(/sha256\s+"[^"]+"/, `sha256 "${sha256}"`);

  if (updatedContent === currentContent) {
    console.log(`Cask is already up to date with version ${cleanVersion}.`);
    return;
  }

  console.log(`Updating ${path} to version ${cleanVersion} with sha256 ${sha256}...`);
  await githubRequest('PUT', `/repos/${repo}/contents/${path}`, {
    message: `bump: limen cask to v${cleanVersion}`,
    content: Buffer.from(updatedContent).toString('base64'),
    sha: fileData.sha,
  });

  console.log(`✅ Successfully updated Homebrew tap to v${cleanVersion}!`);
}

run().catch((err) => {
  console.error('Error updating Homebrew tap:', err);
  process.exit(1);
});
