import { DatabaseSync } from "node:sqlite";
import fs from "node:fs";
import path from "node:path";

const REPO = process.cwd();
const OUT = path.join(REPO, "repowiki");
const SRC =
  process.argv[2] ||
  "C:/Users/Ryukiq/.qoder/qoder-knowledge/knowledge-Moriq-c51f07cf0d0afc13/wiki/wiki.v1.db";

// Read a copy: opening the live store write-mode would checkpoint its WAL.
const TMP = fs.mkdtempSync(path.join(process.env.TEMP || "/tmp", "repowiki-export-"));
for (const suffix of ["", "-wal", "-shm"]) fs.copyFileSync(SRC + suffix, path.join(TMP, path.basename(SRC) + suffix));
const db = new DatabaseSync(path.join(TMP, path.basename(SRC)));
const part = db.prepare("select * from partition limit 1").get();
const snapshot = part.published_snapshot_id;
const rows = db
  .prepare("select id,parent_id,ordinal,title,meta,content from section where snapshot_id=?")
  .all(snapshot);

const kids = new Map();
for (const s of rows) {
  if (!kids.has(s.parent_id)) kids.set(s.parent_id, []);
  kids.get(s.parent_id).push(s);
}
for (const list of kids.values()) list.sort((a, b) => a.ordinal - b.ordinal);

const nameOf = (s) => JSON.parse(s.meta || "{}").sectionName || s.title;

function safeName(name) {
  const cleaned = name
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, "-")
    .replace(/[. ]+$/g, "")
    .trim();
  if (!cleaned) throw new Error("empty file name for section " + name);
  return cleaned;
}

function encodeLink(p) {
  return p
    .split("/")
    .map((seg) => seg.replace(/[%#? ]/g, (c) => ({ "%": "%25", "#": "%23", "?": "%3F", " ": "%20" })[c]))
    .join("/");
}

function rewriteLinks(content, mdFileDir) {
  return content.replace(/file:\/\/([^\s<>"'`)\]]+)/g, (_m, ref) => {
    const hashAt = ref.indexOf("#");
    const rel = (hashAt === -1 ? ref : ref.slice(0, hashAt)).replace(/\\/g, "/");
    const frag = hashAt === -1 ? "" : ref.slice(hashAt);
    const abs = path.resolve(REPO, rel.split("/").join(path.sep));
    let relTo = path.relative(path.resolve(mdFileDir), abs).split(path.sep).join("/");
    if (!relTo.startsWith(".")) relTo = "./" + relTo;
    return encodeLink(relTo) + frag;
  });
}

let fileCount = 0;
let linkCount = 0;
const indexLines = [];

function walk(list, dirParts, depth) {
  for (const s of list) {
    const name = safeName(nameOf(s));
    const children = kids.get(s.id) || [];
    const filePath = path.join(
      OUT,
      ...dirParts,
      ...(children.length ? [name, "README.md"] : [name + ".md"]),
    );
    fs.mkdirSync(path.dirname(filePath), { recursive: true });

    const body = rewriteLinks(s.content, path.dirname(filePath));
    linkCount += (s.content.match(/file:\/\/[^\s<>"'`)\]]+/g) || []).length;

    const provenance = [
      `> Repo Wiki 节点 \`${s.title}\``,
      `> 导出自 Qoder 知识库快照 \`${snapshot}\`，代码基线 \`${part.baseline_commit}\`（分支 ${part.branch}，${part.locale}）。`,
      "",
    ].join("\n");
    const out = body.startsWith("# ")
      ? body.replace(/^#([^\n]*)\n/, (_m, t) => `#${t}\n\n${provenance}`)
      : `# ${name}\n\n${provenance}\n${body}`;

    fs.writeFileSync(filePath, out, "utf8");
    fileCount++;
    indexLines.push(
      "  ".repeat(depth) +
        `- [${nameOf(s)}](${encodeLink(path.relative(OUT, filePath).split(path.sep).join("/"))})`,
    );
    if (children.length) walk(children, [...dirParts, name], depth + 1);
  }
}

walk(kids.get("") || [], [], 0);

const index = [
  "# Moriq Space Repo Wiki",
  "",
  "仓库增补：[NAS 图片访问（1.2.0）](NAS%20图片访问.md)。",
  "",
  `Qoder Repo Wiki 的 Markdown 导出：${fileCount} 个页面，目录层级与 wiki 节点树一致。`,
  `正文逐节照搬，仅把 \`file://\` 代码引用改写为指向仓库文件的相对链接（共 ${linkCount} 处）。`,
  "含子节点的页面写成该目录下的 `README.md`。",
  "",
  `- 分区：branch \`${part.branch}\` / locale \`${part.locale}\``,
  `- 快照：\`${snapshot}\``,
  `- 代码基线：\`${part.baseline_commit}\``,
  "",
  "## 目录",
  "",
  ...indexLines,
  "",
].join("\n");
fs.writeFileSync(path.join(OUT, "INDEX.md"), index, "utf8");

db.close();
fs.rmSync(TMP, { recursive: true, force: true });
console.log(`files=${fileCount} links=${linkCount} roots=${(kids.get("") || []).length} totalChars=${rows.reduce((a, r) => a + r.content.length, 0)}`);
