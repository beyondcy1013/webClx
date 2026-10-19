use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

// Codex's status line and idle-composer braille particles are terminal UI, not
// conversation output. Normalize them only when the recognizable Codex footer
// identifies the composer layout; preserve all other programs and pane text.
pub(super) fn terminal_output_fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    let Ok(text) = std::str::from_utf8(bytes) else {
        bytes.hash(&mut hasher);
        return hasher.finish();
    };
    let mut lines: Vec<&str> = text.lines().collect();
    if let Some(footer) = lines.iter().rposition(|line| {
        line.contains(" · Context ") && line.contains(" used · ") && line.contains(" · Fast ")
    }) {
        // The status/quota footer is refreshed by the TUI independently of agent output.
        lines[footer] = "";
        if footer >= 3 {
            let prompt = footer - 2;
            let clean: String = lines[prompt]
                .chars()
                .filter(|ch| !is_particle(*ch))
                .collect();
            if clean
                .trim()
                .strip_prefix('›')
                .is_some_and(|rest| rest.trim() == "Ask Codex to do anything")
                && [prompt - 1, prompt + 1].iter().all(|&row| {
                    lines[row]
                        .chars()
                        .all(|ch| ch.is_whitespace() || is_particle(ch))
                })
            {
                lines[prompt - 1] = "";
                lines[prompt] = "› Ask Codex to do anything";
                lines[prompt + 1] = "";
            }
        }
    }
    lines.hash(&mut hasher);
    hasher.finish()
}

fn is_particle(ch: char) -> bool {
    matches!(ch, '⠁' | '⠂' | '⠄' | '⠈' | '⠐' | '⠠' | '⡀' | '⢀')
}

#[cfg(test)]
mod tests {
    use super::terminal_output_fingerprint as fingerprint;

    fn pane(body: &str, top: &str, prompt: &str, bottom: &str) -> String {
        format!(
            "{body}\n{top}\n{prompt}\n{bottom}\n  gpt-6-astra low · Context 4% used · Fast off\n\n"
        )
    }

    #[test]
    fn idle_particles_do_not_make_viewed_output_new() {
        let still = pane("Completed", " ", "› Ask Codex to do anything", "");
        let first = pane("Completed", " ⠈  ⢀", "›⠁Ask Codex to do anything⡀ ", " ⠠ ⠐ ");
        let next = pane("Completed", "⠠ ⠂", "› Ask Codex to do anything ⠈⠁", "⢀");
        assert_eq!(fingerprint(still.as_bytes()), fingerprint(first.as_bytes()));
        assert_eq!(fingerprint(first.as_bytes()), fingerprint(next.as_bytes()));
        assert_ne!(
            fingerprint(first.as_bytes()),
            fingerprint(next.replace("Completed", "New reply").as_bytes())
        );
        assert_ne!(
            fingerprint(first.as_bytes()),
            fingerprint(
                next.replace("Ask Codex to do anything", "run tests")
                    .as_bytes()
            )
        );
    }

    #[test]
    fn quota_footer_updates_and_idle_particles_are_not_new_output() {
        let still = pane("Completed", "", "› Ask Codex to do anything", "")
            .replace("used · Fast", "used · 5h 9% left · Fast");
        let animated = pane("Completed", "⠁", "› Ask Codex to do anything ⠂", "⡀")
            .replace("used · Fast", "used · 5h 9% left · Fast");
        assert_eq!(fingerprint(still.as_bytes()), fingerprint(animated.as_bytes()));
        // Status/quota changes do not count as conversation output.
        assert_eq!(
            fingerprint(still.as_bytes()),
            fingerprint(still.replace("9% left", "7% left").as_bytes())
        );
        assert_ne!(
            fingerprint(still.as_bytes()),
            fingerprint(still.replace("Completed", "New reply").as_bytes())
        );
    }

    #[test]
    fn braille_in_output_and_other_programs_is_preserved() {
        let first = pane("Result ⠁", "", "› Ask Codex to do anything", "");
        assert_ne!(
            fingerprint(first.as_bytes()),
            fingerprint(first.replace("Result ⠁", "Result ⠂").as_bytes())
        );
        assert_ne!(fingerprint("⠁\n".as_bytes()), fingerprint("⠂\n".as_bytes()));
        let other = first.replace(" · Context ", " · Other ");
        assert_ne!(
            fingerprint(other.as_bytes()),
            fingerprint(other.replace("\n\n›", "\n⠁\n›").as_bytes())
        );
    }
}
