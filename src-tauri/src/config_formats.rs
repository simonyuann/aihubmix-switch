//! Small, conservative editors for native YAML and dotenv configuration.
//! Change a managed block only; validate the full semantic diff before writing.
use crate::types::Result;
use serde_json::{json, Value};

pub fn yaml_parse(raw: &str) -> Result<Value> {
    if raw.len() > 2 * 1024 * 1024 {
        return Err("YAML 配置超过 2 MiB，未修改".into());
    }
    let value: Value = serde_yaml_ng::from_str(raw).map_err(|_| "YAML 配置格式错误，未修改")?;
    if value.is_null() {
        Ok(json!({}))
    } else if value.is_object() {
        Ok(value)
    } else {
        Err("YAML 根节点必须是对象，未修改".into())
    }
}

pub fn lookup<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = if let Some(name) = part.strip_prefix('@') {
            current.as_array()?.iter().find(|v| v["name"] == name)?
        } else {
            current.get(part)?
        };
    }
    Some(current)
}

fn set(value: &mut Value, path: &[String], next: Option<&Value>) -> Result<()> {
    let Some((first, rest)) = path.split_first() else {
        return Err("空配置路径".into());
    };
    if let Some(name) = first.strip_prefix('@') {
        let array = value.as_array_mut().ok_or("配置字段必须是列表")?;
        let index = array.iter().position(|v| v["name"] == name);
        if !rest.is_empty() {
            return Err("不支持的列表路径".into());
        }
        match (index, next) {
            (Some(i), Some(v)) => array[i] = v.clone(),
            (Some(i), None) => {
                array.remove(i);
            }
            (None, Some(v)) => array.push(v.clone()),
            _ => {}
        }
    } else {
        let map = value.as_object_mut().ok_or("配置字段必须是对象")?;
        if rest.is_empty() {
            if let Some(v) = next {
                map.insert(first.clone(), v.clone());
            } else {
                map.remove(first);
            }
        } else if let Some(v) = map.get_mut(first) {
            set(v, rest, next)?;
        } else if next.is_some() {
            let mut v = if rest[0].starts_with('@') {
                json!([])
            } else {
                json!({})
            };
            set(&mut v, rest, next)?;
            map.insert(first.clone(), v);
        }
    }
    Ok(())
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}
fn meaningful(line: &str) -> bool {
    let s = line.trim();
    !s.is_empty() && !s.starts_with('#')
}
fn key(line: &str) -> Option<String> {
    let (name, _) = line.trim_start().split_once(':')?;
    serde_yaml_ng::from_str::<Value>(name)
        .ok()?
        .as_str()
        .map(str::to_owned)
}
fn rendered(value: &Value, depth: usize) -> Result<Vec<String>> {
    let raw = serde_yaml_ng::to_string(value).map_err(|_| "无法生成 YAML 配置")?;
    Ok(raw
        .lines()
        .map(|line| format!("{}{line}\n", " ".repeat(depth)))
        .collect())
}
fn patch_block(
    lines: &mut Vec<String>,
    path: &[String],
    next: Option<&Value>,
    start: usize,
    end: usize,
    depth: usize,
) -> Result<()> {
    let first = &path[0];
    let sequence = first.starts_with('@');
    let mut found = None;
    let mut index = start;
    while index < end {
        if !meaningful(&lines[index]) || indent(&lines[index]) != depth {
            index += 1;
            continue;
        }
        let mut stop = index + 1;
        while stop < end
            && (!meaningful(&lines[stop])
                || indent(&lines[stop]) > depth
                || (!sequence
                    && indent(&lines[stop]) == depth
                    && lines[stop].trim_start().starts_with("- ")))
        {
            stop += 1;
        }
        let mut content_end = stop;
        while content_end > index + 1 && !meaningful(&lines[content_end - 1]) {
            content_end -= 1;
        }
        let matches = if sequence {
            if !lines[index].trim_start().starts_with("- ") {
                return Err("列表不是可安全编辑的块格式".into());
            }
            let snippet = lines[index..content_end].concat();
            let entries: Value =
                serde_yaml_ng::from_str(&snippet).map_err(|_| "无法定位 YAML 列表项")?;
            entries[0]["name"].as_str() == first.strip_prefix('@')
        } else {
            key(&lines[index]).as_ref() == Some(first)
        };
        if matches {
            found = Some((index, content_end));
            break;
        }
        index = stop;
    }
    if let Some((begin, stop)) = found {
        if path.len() == 1 {
            let replacement = match next {
                Some(v) => rendered(
                    &if sequence {
                        json!([v])
                    } else {
                        json!({first:v})
                    },
                    depth,
                )?,
                None => vec![],
            };
            lines.splice(begin..stop, replacement);
        } else {
            let (_, tail) = lines[begin].split_once(':').ok_or("无法定位 YAML 字段")?;
            if !tail.trim().is_empty() && !tail.trim().starts_with('#') {
                // Empty maps/lists can be expanded, but flow style and tags are never rewritten.
                if !["{}", "[]", "null", "~"].contains(&tail.trim()) {
                    return Err(
                        "此 YAML 字段使用行内格式，请先改为块格式再接入；原配置未修改".into(),
                    );
                }
                lines[begin] = format!(
                    "{}{}:\n",
                    " ".repeat(depth),
                    lines[begin].trim_start().split_once(':').unwrap().0
                );
            }
            let child_depth = lines[begin + 1..stop]
                .iter()
                .find(|s| meaningful(s))
                .map(|s| indent(s))
                .unwrap_or(depth + 2);
            if child_depth < depth || (child_depth == depth && !path[1].starts_with('@')) {
                return Err("YAML 缩进不支持安全修改".into());
            }
            patch_block(lines, &path[1..], next, begin + 1, stop, child_depth)?;
            if next.is_none()
                && lines
                    .iter()
                    .skip(begin + 1)
                    .find(|s| meaningful(s))
                    .is_none_or(|s| indent(s) <= depth && !s.trim_start().starts_with("- "))
            {
                lines[begin] = format!(
                    "{}{}: {}\n",
                    " ".repeat(depth),
                    lines[begin].trim_start().split_once(':').unwrap().0,
                    if path[1].starts_with('@') { "[]" } else { "{}" }
                );
            }
        }
    } else if let Some(v) = next {
        let mut subtree = v.clone();
        for part in path[1..].iter().rev() {
            subtree = if part.starts_with('@') {
                json!([subtree])
            } else {
                json!({part:subtree})
            };
        }
        let value = if sequence {
            json!([subtree])
        } else {
            json!({first:subtree})
        };
        let mut at = end;
        while at > start && !meaningful(&lines[at - 1]) {
            at -= 1;
        }
        if at > 0 && !lines[at - 1].ends_with('\n') {
            lines[at - 1].push('\n');
        }
        lines.splice(at..at, rendered(&value, depth)?);
    }
    Ok(())
}

pub fn yaml_edit(raw: &str, edits: &[(Vec<String>, Option<Value>)]) -> Result<String> {
    let mut expected = yaml_parse(raw)?;
    // YAML graph features can cause edits to affect values outside the selected block.
    if raw.contains('\t')
        || raw.lines().any(|line| {
            let text = line.split('#').next().unwrap_or_default();
            [" &", ": *", "<<:", ": !"].iter().any(|v| text.contains(v))
        })
    {
        return Err("YAML 包含锚点、标签或制表符，请手动配置；原文件未修改".into());
    }
    let mut lines: Vec<String> = raw.split_inclusive('\n').map(str::to_owned).collect();
    for (path, value) in edits {
        set(&mut expected, path, value.as_ref())?;
        let end = lines.len();
        patch_block(&mut lines, path, value.as_ref(), 0, end, 0)?;
    }
    let output = lines.concat();
    if yaml_parse(&output)? != expected {
        return Err("无法仅修改托管的 YAML 字段，原文件未修改".into());
    }
    Ok(output)
}

fn env_line(line: &str) -> Option<(&str, &str)> {
    let s = line.trim().strip_prefix("export ").unwrap_or(line.trim());
    if s.starts_with('#') || s.is_empty() {
        return None;
    }
    let (key, value) = s.split_once('=')?;
    let key = key.trim();
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((key, value.trim()))
}
pub fn env_parse(raw: &str) -> Result<Value> {
    let mut map = serde_json::Map::new();
    for line in raw.lines() {
        let Some((key, raw)) = env_line(line) else {
            continue;
        };
        let value = if raw.starts_with('"') {
            let mut stream = serde_json::Deserializer::from_str(raw).into_iter::<String>();
            let value = stream
                .next()
                .ok_or("环境变量双引号未闭合")?
                .map_err(|_| "环境变量双引号格式不支持安全修改")?;
            let tail = raw[stream.byte_offset()..].trim();
            if !tail.is_empty() && !tail.starts_with('#') {
                return Err("环境变量引号后包含无法识别的内容".into());
            }
            value
        } else if raw.starts_with('\'') {
            let (value, tail) = raw
                .strip_prefix('\'')
                .and_then(|s| s.split_once('\''))
                .ok_or("环境变量单引号未闭合")?;
            if !tail.trim().is_empty() && !tail.trim().starts_with('#') {
                return Err("环境变量引号后包含无法识别的内容".into());
            }
            value.into()
        } else {
            raw.split(" #").next().unwrap_or_default().trim().into()
        };
        map.insert(key.into(), json!(value));
    }
    Ok(Value::Object(map))
}
pub fn env_edit(raw: &str, edits: &[(Vec<String>, Option<Value>)]) -> Result<String> {
    let mut output = raw.to_owned();
    for (path, next) in edits {
        if path.len() != 1 {
            return Err("无效环境变量路径".into());
        }
        let key = &path[0];
        let mut lines: Vec<String> = output
            .split_inclusive('\n')
            .filter(|line| env_line(line).is_none_or(|(name, _)| name != key))
            .map(str::to_owned)
            .collect();
        if let Some(value) = next {
            let value = value.as_str().ok_or("环境变量必须是文本")?;
            if value.contains(['\r', '\n', '\'', '$', '\0']) {
                return Err("环境变量包含不支持的特殊字符，原配置未修改".into());
            }
            if let Some(last) = lines.last_mut() {
                if !last.ends_with('\n') {
                    last.push('\n');
                }
            }
            lines.push(format!("{key}='{value}'\n"));
        }
        output = lines.concat();
    }
    env_parse(&output)?;
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn yaml_edits_only_target_blocks_and_preserves_other_entries_comments() {
        let raw = "# header\nmodel:\n  default: old\n  max_turns: 50 # keep\ncustom_providers:\n  - name: other\n    api_key: 'untouched' # original\n# footer\n";
        let next = yaml_edit(
            raw,
            &[
                (
                    p(&["custom_providers", "@csmy"]),
                    Some(json!({"name":"csmy","api_key":"new","model":"m"})),
                ),
                (p(&["model", "default"]), Some(json!("m"))),
            ],
        )
        .unwrap();
        assert!(next.contains("api_key: 'untouched' # original"));
        assert!(next.contains("max_turns: 50 # keep"));
        assert!(next.contains("# footer"));
        let restored = yaml_edit(&next, &[(p(&["custom_providers", "@csmy"]), None)]).unwrap();
        assert_eq!(
            yaml_parse(&restored).unwrap()["custom_providers"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn yaml_refuses_ambiguous_or_graph_edits_without_rewriting() {
        for raw in [
            "providers: {other: {key: old}}\n",
            "providers:\n  old: &x\n    key: old\n",
            "providers:\n  csmy: {}\n  csmy: {}\n",
        ] {
            assert!(yaml_edit(
                raw,
                &[(p(&["providers", "csmy"]), Some(json!({"key":"new"})))]
            )
            .is_err());
        }
    }
    #[test]
    fn env_handles_duplicates_exports_and_preserves_unmanaged_text() {
        let raw =
            "# keep\nexport GEMINI_API_KEY=old\nOTHER=untouched # keep\nGEMINI_API_KEY=duplicate\n";
        let next = env_edit(raw, &[(p(&["GEMINI_API_KEY"]), Some(json!("new")))]).unwrap();
        assert!(next.contains("OTHER=untouched # keep"));
        assert_eq!(next.matches("GEMINI_API_KEY=").count(), 1);
        assert_eq!(env_parse(&next).unwrap()["GEMINI_API_KEY"], "new");
    }
}
