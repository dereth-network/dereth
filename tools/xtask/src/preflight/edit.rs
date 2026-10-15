//! The one setting the preflight changes in a world's configuration: the hub it links to. Each
//! edit keeps every other line as it was, and reads back the value it replaced, so the original
//! can be put back.

/// Empyrean's `[federation] hub_url` in `toml` set to `url`: the text, and the value it had (none
/// when it was unset, which is the network's own hub).
pub fn empyrean_hub(toml: &str, url: &str) -> Result<(String, Option<String>), String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_federation = false;
    let mut seen_section = false;
    let mut old = None;
    let mut done = false;
    let line = format!("hub_url = \"{}\"", escape_toml(url));
    for l in toml.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if in_federation && !done {
                // At the end of the section's own lines, before the blank lines that close it.
                let blanks = out.iter().rev().take_while(|l| l.trim().is_empty()).count();
                out.insert(out.len() - blanks, line.clone());
                done = true;
            }
            in_federation = t == "[federation]";
            seen_section |= in_federation;
        } else if in_federation && !done && key_of(t) == Some("hub_url") {
            old = value_of(t);
            out.push(line.clone());
            done = true;
            continue;
        }
        out.push(l.to_owned());
    }
    if !done {
        if !seen_section {
            out.push(String::new());
            out.push("[federation]".to_owned());
        }
        out.push(line);
    }
    let mut text = out.join("\n");
    if toml.ends_with('\n') || !seen_section {
        text.push('\n');
    }
    Ok((text, old))
}

/// Empyrean's `[federation] hub_url` put back to `old`: set to it, or taken out when it was unset.
pub fn empyrean_hub_back(toml: &str, old: Option<&str>) -> Result<String, String> {
    match old {
        Some(url) => empyrean_hub(toml, url).map(|(t, _)| t),
        None => {
            let mut in_federation = false;
            let kept: Vec<&str> = toml
                .lines()
                .filter(|l| {
                    let t = l.trim();
                    if t.starts_with('[') {
                        in_federation = t == "[federation]";
                    }
                    !(in_federation && key_of(t) == Some("hub_url"))
                })
                .collect();
            let mut text = kept.join("\n");
            if toml.ends_with('\n') {
                text.push('\n');
            }
            Ok(text)
        }
    }
}

/// The mod's `HubUrl` in its `Settings.json` text set to `url`: the text, and the value it had.
pub fn ace_hub(json: &str, url: &str) -> Result<(String, Option<String>), String> {
    let mut value = settings(json)?;
    let map = value
        .as_object_mut()
        .ok_or("Settings.json is not an object")?;
    let old = map
        .get("HubUrl")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
    map.insert("HubUrl".to_owned(), serde_json::Value::from(url));
    // A dev hub reached on this machine is plain ws://, which the mod takes only when told to.
    if url.starts_with("ws://") {
        map.insert("AllowLocalWs".to_owned(), serde_json::Value::from(true));
    }
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    Ok((format!("{text}\n"), old))
}

/// The mod's `HubUrl` put back to `old`: set to it, or taken out when it was unset.
pub fn ace_hub_back(json: &str, old: Option<&str>) -> Result<String, String> {
    let mut value = settings(json)?;
    let map = value
        .as_object_mut()
        .ok_or("Settings.json is not an object")?;
    match old {
        Some(url) => {
            map.insert("HubUrl".to_owned(), serde_json::Value::from(url));
        }
        None => {
            map.remove("HubUrl");
        }
    }
    let text = serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?;
    Ok(format!("{text}\n"))
}

/// The mod's `Settings.json` text read; an empty file is one that sets nothing.
fn settings(json: &str) -> Result<serde_json::Value, String> {
    if json.trim().is_empty() {
        return Ok(serde_json::Value::Object(serde_json::Map::new()));
    }
    serde_json::from_str(json).map_err(|e| format!("Settings.json: {e}"))
}

/// The key of a TOML line `key = value`, if it is one.
fn key_of(line: &str) -> Option<&str> {
    if line.starts_with('#') {
        return None;
    }
    let (k, _) = line.split_once('=')?;
    Some(k.trim())
}

/// The string value of a TOML line `key = "value"`.
fn value_of(line: &str) -> Option<String> {
    let (_, v) = line.split_once('=')?;
    let v = v.trim();
    let v = v.split(" #").next().unwrap_or(v).trim();
    let inner = v.strip_prefix('"')?.strip_suffix('"')?;
    Some(inner.replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn escape_toml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
