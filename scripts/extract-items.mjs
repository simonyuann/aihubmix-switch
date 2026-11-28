#!/usr/bin/env node
// 治理工具:按名称把顶层项(fn/const/struct + 前置 cfg/doc 注释)从 misc.rs 搬到目标模块。
// 用法: node extract-items.mjs <misc.rs> <target.rs> <name1,name2,...>
// 顶层项边界: 从签名行向后找第一个列 0 的 "}"(rustfmt 保证);向前收连续的 /// 与 #[...] 行。
import fs from "node:fs";

const [, , miscPath, targetPath, namesArg] = process.argv;
if (!miscPath || !targetPath || !namesArg) {
  console.error("usage: node extract-items.mjs <misc.rs> <target.rs> <names>");
  process.exit(1);
}
const names = namesArg.split(",").map((s) => s.trim()).filter(Boolean);
const lines = fs.readFileSync(miscPath, "utf8").split("\n");
const itemRe =
  /^(pub(\(crate\))? )?(async )?fn (NAME)|^(pub(\(crate\))? )?const (NAME)|^(pub(\(crate\))? )?static (NAME)|^(pub(\(crate\))? )?struct (NAME)/;

function itemName(line) {
  const m = line.match(
    /^(?:pub(?:\(crate\))? )?(?:async )?fn (\w+)|^(?:pub(?:\(crate\))? )?const (\w+)|^(?:pub(?:\(crate\))? )?static (\w+)|^(?:pub(?:\(crate\))? )?(?:struct|enum) (\w+)/,
  );
  if (!m) return null;
  return m.slice(1).find(Boolean);
}

function isConstLike(line) {
  return /^(?:pub(?:\(crate\))? )?(?:const|static) /.test(line);
}

// 收集所有顶层项(签名行索引 → {name, bodyStart, bodyEnd, prefixStart})
const items = [];
for (let i = 0; i < lines.length; i++) {
  const name = itemName(lines[i]);
  if (!name) continue;
  // 只匹配顶层(前一非空行不以缩进续行特征)——签名行本身列 0
  if (lines[i].startsWith(" ") || lines[i].startsWith("\t")) continue;
  // 向后找边界:const/static 以第一个以 ";" 结尾的行为止;其余找列 0 的 "}"
  let end = -1;
  if (isConstLike(lines[i])) {
    for (let j = i; j < lines.length; j++) {
      if (lines[j].trimEnd().endsWith(";")) {
        end = j;
        break;
      }
    }
  } else if (lines[i].trimEnd().endsWith(";")) {
    // 单行元组 struct/enum 以分号结尾
    end = i;
  } else {
    for (let j = i + 1; j < lines.length; j++) {
      if (lines[j] === "}") {
        end = j;
        break;
      }
    }
  }
  if (end < 0) throw new Error(`no closing brace for ${name} @${i + 1}`);
  // 向前收连续的属性/文档/空行边界(停在第一个非 #[ / 非 /// 行)
  let start = i;
  while (start > 0 && (/^\s*(#\[|\/\/\/|\/\/!)/.test(lines[start - 1]) || lines[start - 1] === "")) {
    // 空行只在紧邻属性/文档链中间时收,避免吞掉上一个项后的多个空行
    if (lines[start - 1] === "" && !/^\s*(#\[|\/\/\/|\/\/!)/.test(lines[start - 2] ?? "")) break;
    start--;
  }
  items.push({ name, start, end, sig: i });
}

const wanted = [];
const used = new Set();
for (const name of names) {
  const matches = items.filter((it) => it.name === name);
  if (matches.length === 0) {
    console.error(`WARN: ${name} not found in ${miscPath}`);
    continue;
  }
  for (const it of matches) {
    wanted.push(it);
    for (let k = it.start; k <= it.end; k++) used.add(k);
  }
}
wanted.sort((a, b) => a.start - b.start);

// 检查区间重叠(嵌套顶层项不该发生)
for (let i = 1; i < wanted.length; i++) {
  if (wanted[i].start <= wanted[i - 1].end) {
    throw new Error(`overlap: ${wanted[i - 1].name} & ${wanted[i].name}`);
  }
}

const extracted = wanted.map((it) => lines.slice(it.start, it.end + 1).join("\n")).join("\n\n");
const remaining = lines.filter((_, idx) => !used.has(idx)).join("\n");

fs.writeFileSync(targetPath, extracted + "\n", "utf8");
fs.writeFileSync(miscPath, remaining, "utf8");
console.log(
  `extracted ${wanted.length} items (${[...new Set(wanted.map((w) => w.name))].join(", ")}) -> ${targetPath}`,
);
