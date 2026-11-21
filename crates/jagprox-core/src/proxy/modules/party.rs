//! `/psc`: sends `/party list`, collects the member names from the reply (or gives up
//! after 5 s), then shows everyone's stats. Port of `handlePartyStatCheck`.

use std::time::Duration;

use super::{ServerPacket, State};
use crate::chat_parsers::parse_party_line;
use crate::proxy::lookups;
use crate::proxy::session::Session;

pub struct PartyCapture {
    id: u64,
    members: Vec<String>,
    capturing: bool,
    not_in_party: bool,
    mode: Option<String>,
}

/// Starts a party stat check.
pub fn start(s: &Session, mode: Option<String>) {
    s.chat("§eRequesting party member list...");
    let id = {
        let mut st = s.state();
        let id = st.party.as_ref().map_or(1, |p| p.id + 1);
        st.party = Some(PartyCapture {
            id,
            members: Vec::new(),
            capturing: false,
            not_in_party: false,
            mode,
        });
        id
    };
    s.send_server_chat("/party list");
    let session = s.clone();
    s.spawn(async move {
        // Always stop listening, even if Hypixel never answers.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let capture = {
            let mut st = session.state();
            match &st.party {
                Some(p) if p.id == id => st.party.take(),
                _ => None,
            }
        };
        if let Some(capture) = capture {
            finish(&session, capture);
        }
    });
}

fn finish(s: &Session, capture: PartyCapture) {
    if !capture.members.is_empty() {
        let session = s.clone();
        s.spawn(async move { lookups::party_stats(&session, capture.members, capture.mode).await });
    } else if !capture.not_in_party {
        s.chat("§cCould not find any party members.");
    }
}

/// What one chat line does to a running capture.
#[derive(Debug, PartialEq)]
enum Step {
    Continue,
    Done,
    NotInParty,
}

fn step(capture: &mut PartyCapture, clean: &str) -> Step {
    if clean.contains("You are not currently in a party.") {
        capture.not_in_party = true;
        return Step::NotInParty;
    }
    if clean.starts_with("Party Members (") || clean.starts_with("Party Leader:") {
        capture.capturing = true;
    }
    if capture.capturing {
        if let Some(names) = parse_party_line(clean) {
            for name in names {
                if !capture.members.contains(&name) {
                    capture.members.push(name);
                }
            }
        }
        if clean.starts_with("-----") && !capture.members.is_empty() {
            return Step::Done;
        }
    }
    Step::Continue
}

pub fn handle(st: &mut State, s: &Session, packet: &ServerPacket) {
    let ServerPacket::Chat(chat) = packet else {
        return;
    };
    let Some(clean) = &chat.clean else { return };
    let Some(capture) = &mut st.party else { return };
    match step(capture, clean) {
        Step::Continue => return,
        Step::NotInParty => s.chat("§cYou are not in a party."),
        Step::Done => {}
    }
    if let Some(capture) = st.party.take() {
        finish(s, capture);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capture() -> PartyCapture {
        PartyCapture {
            id: 1,
            members: Vec::new(),
            capturing: false,
            not_in_party: false,
            mode: None,
        }
    }

    #[test]
    fn collects_members_until_the_closing_line() {
        let mut c = capture();
        // The opening divider comes before any member, so it must not end the capture.
        assert_eq!(
            step(&mut c, "-----------------------------"),
            Step::Continue
        );
        assert_eq!(step(&mut c, "Party Members (3)"), Step::Continue);
        assert_eq!(step(&mut c, "Party Leader: [MVP+] Alice ●"), Step::Continue);
        assert_eq!(
            step(&mut c, "Party Members: Bob ● [VIP] Carol ●"),
            Step::Continue
        );
        assert_eq!(step(&mut c, "-----------------------------"), Step::Done);
        assert_eq!(c.members, ["Alice", "Bob", "Carol"]);
    }

    #[test]
    fn not_in_a_party() {
        let mut c = capture();
        assert_eq!(
            step(&mut c, "You are not currently in a party."),
            Step::NotInParty
        );
        assert!(c.not_in_party && c.members.is_empty());
    }
}
