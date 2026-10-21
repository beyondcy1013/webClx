pub const ICON_COUNT: usize = 8;

pub fn icon_index(host_name: &str) -> usize {
    // FNV-1a is stable across processes and platforms, unlike DefaultHasher.
    let hash = host_name
        .trim()
        .to_ascii_lowercase()
        .bytes()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
        });
    let mixed = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    let mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d049bb133111eb);
    ((mixed ^ (mixed >> 31)) as usize) % ICON_COUNT
}

pub fn icon_path(host_name: &str) -> String {
    format!("favicon-{}.svg", icon_index(host_name))
}

#[cfg(test)]
mod tests {
    use super::{ICON_COUNT, icon_index, icon_path};

    #[test]
    fn selection_is_stable_and_case_insensitive() {
        assert_eq!(icon_index("WebCLX-US"), icon_index("webclx-us"));
        assert_eq!(icon_index(" webclx-us "), icon_index("webclx-us"));
        assert_eq!(icon_path("webclx-us"), icon_path("webclx-us"));
    }

    #[test]
    fn selection_stays_within_palette() {
        for host_name in ["localhost", "webclx-cn", "webclx-us", "server-42"] {
            assert!(icon_index(host_name) < ICON_COUNT);
        }
    }

    #[test]
    fn known_deployment_hosts_receive_distinct_icons() {
        let indexes = [
            icon_index("VM-0-7-opencloudos"),
            icon_index("VM-0-8-ubuntu"),
            icon_index("lavm-rzm7363tfk"),
            icon_index("Z2Pro-Y4KG"),
        ];
        let count = indexes.len();
        let unique = indexes
            .into_iter()
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(unique.len(), count);
    }
}
