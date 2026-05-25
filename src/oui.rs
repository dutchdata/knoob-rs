use std::collections::HashMap;
use std::fs;

pub type OuiMap = HashMap<[u8; 3], String>;

/// Parse Wireshark's `manuf` format. Lines look like:
///   00:00:00 \tXerox \tXerox Corporation
///   00:01:42 \tCisco \tCisco Systems, Inc
/// Ignores blank lines, comments (#), and longer prefix matches (e.g. /28).
/// We only take the short name (column 2).
pub fn load(path: &str) -> Result<OuiMap, Box<dyn std::error::Error>> {
    let text = fs::read_to_string(path)?;
    let mut map = OuiMap::with_capacity(50_000);

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') { continue; }

        // tab-separated; first col is OUI, second is short name
        let mut parts = line.split('\t');
        let oui_str = match parts.next() { Some(s) => s.trim(), None => continue };
        let short   = match parts.next() { Some(s) => s.trim(), None => continue };

        // skip prefix-length entries (e.g. "00:55:DA:00:00:00/28")
        if oui_str.contains('/') { continue; }

        let bytes: Vec<&str> = oui_str.split(':').collect();
        if bytes.len() != 3 { continue; }

        let mut oui = [0u8; 3];
        let mut ok = true;
        for i in 0..3 {
            match u8::from_str_radix(bytes[i], 16) {
                Ok(b) => oui[i] = b,
                Err(_) => { ok = false; break; }
            }
        }
        if !ok { continue; }

        map.insert(oui, short.to_string());
    }

    Ok(map)
}

pub fn lookup(map: &OuiMap, mac: &[u8; 6]) -> Option<String> {
    let oui = [mac[0], mac[1], mac[2]];
    map.get(&oui).cloned()
}
