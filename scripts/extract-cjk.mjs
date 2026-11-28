// 提取 ToolPage.tsx 中含中文的字符串字面量(去重)
import fs from "node:fs";
const src = fs.readFileSync("src/components/aihubmix/ToolPage.tsx", "utf8");
const re2 = /(['"])((?:\\.|(?!\1).)*[\u4e00-\u9fff](?:\\.|(?!\1).)*)\1/g;
const found = new Map();
let m;
while ((m = re2.exec(src)) !== null) {
  const text = m[2];
  if (!found.has(text)) {
    found.set(text, { count: 0, first: src.slice(0, m.index).split("\n").length });
  }
  found.get(text).count++;
}
console.log("唯一字符串数:", found.size);
let i = 0;
for (const [text, info] of found) {
  i++;
  console.log(`${i}. [${info.count}x L${info.first}] ${text.slice(0, 70)}`);
}
