use crate::network::NodeInfo;

const MICRON_EXTENSIONS: &[&str] = &["", "mu", "md", "micron"];

#[derive(Debug, Clone)]
pub enum LinkAction {
    Navigate {
        node: NodeInfo,
        path: String,
    },
    Download {
        node: NodeInfo,
        path: String,
        filename: String,
    },
    Lxmf {
        hash: [u8; 16],
    },
    Unknown {
        url: String,
    },
}

pub fn resolve_link(
    link_url: &str,
    current_node: Option<&NodeInfo>,
    known_nodes: &[NodeInfo],
) -> LinkAction {
    if let Some(hash) = parse_lxmf_link(link_url) {
        return LinkAction::Lxmf { hash };
    }

    if let Some((node, path)) = resolve_node_link(link_url, current_node, known_nodes) {
        if is_download_path(&path) {
            let filename = extract_filename(&path);
            return LinkAction::Download {
                node,
                path,
                filename,
            };
        }
        return LinkAction::Navigate { node, path };
    }

    LinkAction::Unknown {
        url: link_url.to_string(),
    }
}

fn parse_lxmf_link(url: &str) -> Option<[u8; 16]> {
    let rest = url.strip_prefix("lxmf@")?;

    if rest.len() != 32 {
        return None;
    }

    let hash_bytes = hex::decode(rest).ok()?;
    if hash_bytes.len() != 16 {
        return None;
    }

    let mut hash = [0u8; 16];
    hash.copy_from_slice(&hash_bytes);
    Some(hash)
}

fn resolve_node_link(
    link_url: &str,
    current_node: Option<&NodeInfo>,
    known_nodes: &[NodeInfo],
) -> Option<(NodeInfo, String)> {
    if let Some(rest) = link_url.strip_prefix(':') {
        let node = current_node?.clone();
        let path = normalize_path(rest);
        return Some((node, path));
    }

    if link_url.contains(':') {
        let parts: Vec<&str> = link_url.splitn(2, ':').collect();
        if parts.len() == 2 && parts[0].len() == 32 {
            let hash_hex = parts[0];
            let path = parts[1].to_string();

            if let Ok(hash_bytes) = hex::decode(hash_hex) {
                if hash_bytes.len() == 16 {
                    let mut hash = [0u8; 16];
                    hash.copy_from_slice(&hash_bytes);

                    let node = known_nodes
                        .iter()
                        .find(|n| n.hash == hash)
                        .cloned()
                        .or_else(|| current_node.filter(|n| n.hash == hash).cloned());

                    if let Some(node) = node {
                        return Some((node, normalize_path(&path)));
                    }
                }
            }
        }
    }

    let node = current_node?.clone();
    let path = normalize_path(link_url);
    Some((node, path))
}

fn normalize_path(path: &str) -> String {
    if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{}", path)
    }
}

fn is_download_path(path: &str) -> bool {
    let path_lower = path.to_lowercase();

    let ext = path_lower.rsplit('.').next().unwrap_or("");

    if ext.is_empty() {
        return false;
    }

    !MICRON_EXTENSIONS.contains(&ext)
}

fn extract_filename(path: &str) -> String {
    path.rsplit('/').next().unwrap_or("download").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(name: &str, hash: [u8; 16]) -> NodeInfo {
        NodeInfo {
            name: name.to_string(),
            hash,
            identity: crate::network::IdentityInfo {
                public_key: [0; 32],
                signature: [0; 64],
            },
        }
    }

    #[test]
    fn test_lxmf_link() {
        let url = "lxmf@0123456789abcdef0123456789abcdef";
        let action = resolve_link(url, None, &[]);
        assert!(matches!(action, LinkAction::Lxmf { .. }));
    }

    #[test]
    fn test_relative_path() {
        let node = make_node("test", [1; 16]);
        let action = resolve_link("/page", Some(&node), &[]);
        assert!(matches!(action, LinkAction::Navigate { path, .. } if path == "/page"));
    }

    #[test]
    fn test_download_detection() {
        let node = make_node("test", [1; 16]);

        let action = resolve_link("/file.pdf", Some(&node), &[]);
        assert!(matches!(action, LinkAction::Download { filename, .. } if filename == "file.pdf"));

        let action = resolve_link("/page.mu", Some(&node), &[]);
        assert!(matches!(action, LinkAction::Navigate { .. }));

        let action = resolve_link("/page", Some(&node), &[]);
        assert!(matches!(action, LinkAction::Navigate { .. }));
    }

    #[test]
    fn test_colon_prefix() {
        let node = make_node("test", [1; 16]);
        let action = resolve_link(":/other", Some(&node), &[]);
        assert!(matches!(action, LinkAction::Navigate { path, .. } if path == "/other"));
    }
}
