use std::collections::BTreeMap;

/// A configuration layer: a map from relative path to file content.
///
/// Each key is the relative path of a managed file (e.g.
/// `agents/security.md`) and the value is its full content. `BTreeMap`
/// gives deterministic iteration order, which is required so that
/// daemons running on the same layers produce byte-identical output.
pub type ConfigLayer = BTreeMap<String, String>;

/// Merge three configuration layers with `user > team > org` precedence.
///
/// - Entries with the same key in multiple layers: the higher layer wins.
/// - Entries with distinct keys: all included (additive).
#[must_use]
pub fn merge(org: &ConfigLayer, team: &ConfigLayer, user: &ConfigLayer) -> ConfigLayer {
    let mut out = org.clone();
    out.extend(team.iter().map(|(k, v)| (k.clone(), v.clone())));
    out.extend(user.iter().map(|(k, v)| (k.clone(), v.clone())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_of_empty_layers_is_empty() {
        let empty: ConfigLayer = BTreeMap::new();
        let result = merge(&empty, &empty, &empty);
        assert!(result.is_empty());
    }

    #[test]
    fn merge_combines_distinct_keys_from_all_layers() {
        let org = layer([("agents/security.md", "org-sec")]);
        let team = layer([("commands/test.md", "team-test")]);
        let user = layer([("agents/me.md", "user-me")]);

        let result = merge(&org, &team, &user);

        assert_eq!(result.len(), 3);
        assert_eq!(result["agents/security.md"], "org-sec");
        assert_eq!(result["commands/test.md"], "team-test");
        assert_eq!(result["agents/me.md"], "user-me");
    }

    #[test]
    fn user_overrides_team_overrides_org_on_same_key() {
        let org = layer([("agents/security.md", "org-version")]);
        let team = layer([("agents/security.md", "team-version")]);
        let user = layer([("agents/security.md", "user-version")]);

        let result = merge(&org, &team, &user);

        assert_eq!(result.len(), 1);
        assert_eq!(result["agents/security.md"], "user-version");
    }

    #[test]
    fn team_overrides_org_when_user_absent() {
        let org = layer([("agents/security.md", "org-version")]);
        let team = layer([("agents/security.md", "team-version")]);
        let user = layer([]);

        let result = merge(&org, &team, &user);

        assert_eq!(result["agents/security.md"], "team-version");
    }

    fn layer<const N: usize>(entries: [(&str, &str); N]) -> ConfigLayer {
        entries
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    proptest::proptest! {
        /// User layer entries are always preserved verbatim in the merged result.
        #[test]
        fn user_entries_always_win(
            org in arb_layer(),
            team in arb_layer(),
            user in arb_layer(),
        ) {
            let result = merge(&org, &team, &user);
            for (k, v) in &user {
                proptest::prop_assert_eq!(result.get(k), Some(v));
            }
        }

        /// Every key that exists in any input layer is present in the merged result.
        #[test]
        fn merged_keys_are_the_union(
            org in arb_layer(),
            team in arb_layer(),
            user in arb_layer(),
        ) {
            let result = merge(&org, &team, &user);
            for k in org.keys().chain(team.keys()).chain(user.keys()) {
                proptest::prop_assert!(result.contains_key(k));
            }
        }

        /// Merge is deterministic: same inputs, same output, byte-identical.
        #[test]
        fn merge_is_deterministic(
            org in arb_layer(),
            team in arb_layer(),
            user in arb_layer(),
        ) {
            proptest::prop_assert_eq!(merge(&org, &team, &user), merge(&org, &team, &user));
        }
    }

    fn arb_layer() -> impl proptest::strategy::Strategy<Value = ConfigLayer> {
        use proptest::collection::btree_map;
        use proptest::string::string_regex;

        // Realistic-looking keys (path-like, alphanumeric + slashes) keep the
        // search space tight while still finding override collisions.
        let key = string_regex("[a-z]{1,8}/[a-z]{1,8}\\.md").unwrap();
        let value = string_regex("[a-z]{0,16}").unwrap();
        btree_map(key, value, 0..6)
    }
}
