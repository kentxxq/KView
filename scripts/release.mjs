import { readFileSync, writeFileSync, readdirSync, mkdirSync, copyFileSync } from "node:fs";
import { join, basename } from "node:path";
import { execFileSync } from "node:child_process";
import { createHash } from "node:crypto";

const readJson = (path) => JSON.parse(readFileSync(path, "utf8"));
const tag = process.env.GITHUB_REF_NAME;
const repository = process.env.GITHUB_REPOSITORY;
const sha = process.env.GITHUB_SHA;
const releaseDirectory = process.env.RUNNER_TEMP ? join(process.env.RUNNER_TEMP, "kview-release") : "debug/tmpfile-AI-release-stage";
const stable = /^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const platformFiles = {
  "windows-x64": { target: "windows-x86_64", installer: "KView-windows-x64.exe", bundle: "nsis", extension: ".exe" },
  "macos-arm64": { target: "darwin-aarch64", installer: "KView-macos-arm64.dmg", bundle: "dmg", extension: ".dmg" },
  "macos-x64": { target: "darwin-x86_64", installer: "KView-macos-x64.dmg", bundle: "dmg", extension: ".dmg" },
};

function run(command, args) {
  // 不转发可能含认证信息的子进程错误输出。
  try { return execFileSync(command, args, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim(); }
  catch (error) { throw new Error(`${command} ${args.slice(0, 2).join(" ")} 执行失败（退出码 ${error.status ?? "未知"}）`); }
}

function validateVersion() {
  if (!stable.test(tag ?? "")) throw new Error("发布标签必须为 vX.Y.Z，不支持预发布版本");
  const version = tag.slice(1);
  const cargoVersion = readFileSync("src-tauri/Cargo.toml", "utf8").match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  if ([readJson("package.json").version, readJson("src-tauri/tauri.conf.json").version, cargoVersion].some((value) => value !== version)) {
    throw new Error("标签与前端、Rust、Tauri 版本不一致");
  }
  return version;
}

function compare(left, right) {
  const a = left.slice(1).split(".").map(BigInt);
  const b = right.slice(1).split(".").map(BigInt);
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i] ? 1 : -1;
  }
  return 0;
}

function listReleases() {
  return JSON.parse(run("gh", ["api", `repos/${repository}/releases`, "--paginate", "--slurp"])).flat();
}

function validate() {
  validateVersion();
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository ?? "") || !/^[a-f\d]{40}$/.test(sha ?? "")) {
    throw new Error("缺少有效的 GitHub 仓库或提交信息");
  }
  run("git", ["merge-base", "--is-ancestor", sha, "origin/main"]);
  const published = listReleases().filter((release) => !release.draft && !release.prerelease && stable.test(release.tag_name));
  if (published.some((release) => compare(tag, release.tag_name) < 0)) {
    throw new Error("发布版本必须高于其他已发布正式版本");
  }
  const current = published.find((release) => release.tag_name === tag);
  if (current && current.target_commitish !== sha) throw new Error("已有同版本 Release 指向其他提交，禁止覆盖");
  console.log("版本一致、提交属于 main，且没有更高正式版或冲突的同版本发布。");
}

function singleFile(directory, suffix) {
  const candidates = readdirSync(directory).filter((name) => name.endsWith(suffix));
  if (candidates.length !== 1) throw new Error(`产物数量异常：${directory} (${suffix})`);
  return join(directory, candidates[0]);
}

function stage() {
  const platform = process.env.RELEASE_PLATFORM;
  const config = platformFiles[platform];
  if (!config) throw new Error("不支持的发布平台");
  const releasing = process.env.RELEASE_SIGNED === "true";
  const root = "src-tauri/target/release/bundle";
  mkdirSync(releaseDirectory, { recursive: true });
  const installer = singleFile(join(root, config.bundle), config.extension);
  copyFileSync(installer, join(releaseDirectory, config.installer));
  if (!releasing) return;
  const version = validateVersion();
  const source = platform === "windows-x64" ? installer : singleFile(join(root, "macos"), ".app.tar.gz");
  const asset = platform === "windows-x64" ? config.installer : `KView-${platform}.app.tar.gz`;
  const signature = readFileSync(`${source}.sig`, "utf8").trim();
  if (!signature) throw new Error("缺少更新包签名");
  if (asset !== config.installer) copyFileSync(source, join(releaseDirectory, asset));
  copyFileSync(`${source}.sig`, join(releaseDirectory, `${asset}.sig`));
  writeFileSync(join(releaseDirectory, `${platform}.json`), JSON.stringify({
    version, tag, sha, target: config.target, asset, installer: config.installer, signature,
  }, null, 2));
}

function verifyAssets(releaseTag, assets) {
  // 按标签查询的接口只返回已发布版本，草稿必须从具有写权限的列表中读取。
  const release = listReleases().find((entry) => entry.tag_name === releaseTag);
  if (!release) throw new Error(`未找到 ${releaseTag} 的版本或草稿`);
  if (release.assets.length !== assets.length) throw new Error(`${releaseTag} 发布附件数量不一致`);
  for (const path of assets) {
    const bytes = readFileSync(path);
    const digest = `sha256:${createHash("sha256").update(bytes).digest("hex")}`;
    if (!release.assets.some((asset) => asset.name === basename(path) && asset.size === bytes.length && asset.digest === digest)) {
      throw new Error(`${releaseTag} 发布附件内容不一致，停止同步`);
    }
  }
  return release;
}

function markLatest(versionTag, versionSha) {
  const releases = listReleases();
  const versions = releases.filter((release) => !release.draft && !release.prerelease && stable.test(release.tag_name));
  const version = versions.find((release) => release.tag_name === versionTag);
  if (!version) throw new Error("未找到已公开的正式版本");
  const duplicate = releases.find((release) => release.tag_name === "latest");
  if (duplicate) {
    // 仅清理已经有完整版本存档的重复 Release，不删除 latest 标签。
    const archived = versions.some((entry) => entry.assets.length === duplicate.assets.length
      && duplicate.assets.every((asset) => entry.assets.some((saved) => saved.name === asset.name
        && saved.size === asset.size && saved.digest && saved.digest === asset.digest)));
    if (!archived) throw new Error("latest 附件尚未完整存档，停止清理重复 Release");
  }
  for (const entry of versions) {
    if (entry.tag_name !== versionTag && entry.name !== entry.tag_name) {
      run("gh", ["release", "edit", entry.tag_name, "--title", entry.tag_name, "--latest=false"]);
    }
  }
  run("gh", ["release", "edit", versionTag, "--title", `latest (${versionTag})`, "--latest"]);
  const ref = `repos/${repository}/git/refs/tags/latest`;
  const refs = JSON.parse(run("gh", ["api", `repos/${repository}/git/matching-refs/tags/latest`]));
  const currentRef = refs.find((entry) => entry.ref === "refs/tags/latest");
  if (currentRef) {
    if (currentRef.object.sha !== versionSha) {
      run("gh", ["api", "--method", "PATCH", ref, "-f", `sha=${versionSha}`, "-F", "force=true", "--silent"]);
    }
  } else {
    run("gh", ["api", "--method", "POST", `repos/${repository}/git/refs`, "-f", "ref=refs/tags/latest", "-f", `sha=${versionSha}`, "--silent"]);
  }
  if (duplicate) run("gh", ["release", "delete", "latest", "--yes"]);
  const latest = JSON.parse(run("gh", ["api", `repos/${repository}/releases/latest`]));
  if (latest.tag_name !== versionTag || latest.name !== `latest (${versionTag})`) {
    throw new Error("最新版本的标记或名称不一致，请重试发布任务");
  }
}

function normalize() {
  const versions = listReleases().filter((release) => !release.draft && !release.prerelease && stable.test(release.tag_name));
  const newest = versions.sort((left, right) => compare(right.tag_name, left.tag_name))[0];
  if (!newest) throw new Error("没有已发布的正式版本");
  const versionSha = run("git", ["rev-parse", `${newest.tag_name}^{commit}`]);
  markLatest(newest.tag_name, versionSha);
  console.log(`发布名称已整理：latest (${newest.tag_name})，版本资产及历史标签保留。`);
}

function publish() {
  // 上传前再校验，防止构建期间已有更高版本发布。
  validate();
  const version = validateVersion();
  const platforms = {};
  const assets = new Set();
  for (const [platform, config] of Object.entries(platformFiles)) {
    const fragment = readJson(join(releaseDirectory, `${platform}.json`));
    const expectedAsset = platform === "windows-x64" ? config.installer : `KView-${platform}.app.tar.gz`;
    if (fragment.version !== version || fragment.tag !== tag || fragment.sha !== sha || fragment.target !== config.target
      || fragment.installer !== config.installer || fragment.asset !== expectedAsset) {
      throw new Error("更新产物的版本、平台或提交不一致");
    }
    const signature = readFileSync(join(releaseDirectory, `${fragment.asset}.sig`), "utf8").trim();
    if (!signature || signature !== fragment.signature) throw new Error("更新包签名不一致");
    for (const name of [fragment.asset, fragment.installer, `${fragment.asset}.sig`]) {
      if (readFileSync(join(releaseDirectory, name)).length === 0) throw new Error("发布资产为空");
      assets.add(join(releaseDirectory, name));
    }
    platforms[config.target] = {
      signature,
      url: `https://github.com/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(fragment.asset)}`,
    };
  }
  const existing = listReleases().find((release) => release.tag_name === tag);
  if (!existing) {
    const changelog = readFileSync("CHANGLOG.md", "utf8");
    const heading = `## ${tag} (`;
    const start = changelog.indexOf(heading);
    const notesFile = join(releaseDirectory, "release-notes.md");
    const section = start < 0 ? "" : changelog.slice(start).split(/\r?\n(?=#{1,3} )/)[0].trim();
    const noteArgs = section ? ["--notes-file", notesFile] : ["--generate-notes"];
    if (section) writeFileSync(notesFile, section);
    run("gh", ["release", "create", tag, "--verify-tag", "--target", sha, "--draft", "--title", tag, ...noteArgs]);
  }
  const notes = JSON.parse(run("gh", ["release", "view", tag, "--json", "body"])).body;
  const manifest = join(releaseDirectory, "latest.json");
  const pubDate = new Date(run("git", ["show", "-s", "--format=%cI", sha])).toISOString();
  writeFileSync(manifest, JSON.stringify({ version, notes, pub_date: pubDate, platforms }, null, 2));
  const completeAssets = [...assets, manifest];
  // 已公开版本只校验，不覆盖；允许单独重试 latest 同步。
  if (!existing || existing.draft) {
    run("gh", ["release", "upload", tag, ...completeAssets, "--clobber"]);
  }
  verifyAssets(tag, completeAssets);
  run("gh", ["release", "edit", tag, "--target", sha, "--draft=false", "--prerelease=false", "--latest=false"]);
  markLatest(tag, sha);
  console.log(`${tag} 已发布并显示为 latest (${tag})，latest 标签指向同一提交。`);
}

try {
  const action = { validate, stage, publish, normalize }[process.argv[2]];
  if (!action) throw new Error("未知发布操作");
  action();
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
