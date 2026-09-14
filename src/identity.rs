//! Account identities shared by live traffic, history, storage and cached clients.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;

pub fn canonical_chat_id(jid: &str) -> Cow<'_, str> {
    let Some((user, server)) = jid.split_once('@') else {
        return Cow::Borrowed(jid);
    };
    if !matches!(server, "s.whatsapp.net" | "c.us" | "lid") {
        return Cow::Borrowed(jid);
    }
    let account = if let Some((agent_user, device)) = user.split_once(':') {
        if device.is_empty()
            || !device.bytes().all(|b| b.is_ascii_digit())
            || device.parse::<u16>().is_err()
        {
            return Cow::Borrowed(jid);
        }
        if let Some((account, agent)) = agent_user.split_once('.') {
            if agent.is_empty()
                || !agent.bytes().all(|b| b.is_ascii_digit())
                || agent.parse::<u8>().is_err()
            {
                return Cow::Borrowed(jid);
            }
            account
        } else {
            agent_user
        }
    } else {
        user
    };
    if account.is_empty() || !account.bytes().all(|b| b.is_ascii_digit()) {
        return Cow::Borrowed(jid);
    }
    let server = if server == "c.us" {
        "s.whatsapp.net"
    } else {
        server
    };
    if account == user && jid.ends_with(server) {
        Cow::Borrowed(jid)
    } else {
        Cow::Owned(format!("{account}@{server}"))
    }
}

pub fn phone_number(jid: &str) -> Option<String> {
    let canonical = canonical_chat_id(jid);
    let (user, server) = canonical.split_once('@')?;
    (server == "s.whatsapp.net" && !user.is_empty() && user.bytes().all(|b| b.is_ascii_digit()))
        .then(|| user.to_string())
}

pub fn deserialize_contact_id<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    Ok(canonical_chat_id(&value).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_addresses_resolve_only_within_their_verified_namespace() {
        for input in [
            "447700900123:22@s.whatsapp.net",
            "447700900123:17@s.whatsapp.net",
            "447700900123.0:22@s.whatsapp.net",
            "447700900123:17@c.us",
        ] {
            assert_eq!(canonical_chat_id(input), "447700900123@s.whatsapp.net");
            assert_eq!(phone_number(input).as_deref(), Some("447700900123"));
        }
        assert_eq!(canonical_chat_id("900000000001:22@lid"), "900000000001@lid");
        assert_eq!(phone_number("900000000001:22@lid"), None);
        for input in [
            "447700900123@s.whatsapp.net",
            "123:22@g.us",
            "status@broadcast",
            "123:22@broadcast",
            "123:22@newsletter",
            "123:22@example.test",
            "abc:22@s.whatsapp.net",
            ":22@s.whatsapp.net",
            "123:65536@s.whatsapp.net",
            "123.256:22@s.whatsapp.net",
            "123:+22@s.whatsapp.net",
            "123:22:17@s.whatsapp.net",
        ] {
            assert_eq!(canonical_chat_id(input), input);
        }
    }
}
