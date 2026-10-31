//! Device identity and the `Authorization: MediaBrowser` header.
use crate::error::Error;

/// Names Jellyfin records for this player when QuickConnect starts.
///
/// The server rejects `POST /QuickConnect/Initiate` unless client, device,
/// device id, and version are all non-empty. Build one with [`ClientIdentity::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIdentity {
    client: String,
    device: String,
    device_id: String,
    version: String,
}

impl ClientIdentity {
    /// Identity sent on every request.
    ///
    /// # Errors
    ///
    /// Returns [`Error::InvalidIdentity`] when any field is empty or only whitespace.
    pub fn new(
        client: impl Into<String>,
        device: impl Into<String>,
        device_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Result<Self, Error> {
        let identity = Self {
            client: client.into(),
            device: device.into(),
            device_id: device_id.into(),
            version: version.into(),
        };
        identity.check()?;
        Ok(identity)
    }

    /// Application name, for example `DeskQuest`.
    #[must_use]
    pub fn client(&self) -> &str {
        &self.client
    }

    /// Human-readable device name, for example `Quest 3`.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// Stable id for this install. Jellyfin binds the access token to it.
    #[must_use]
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Application version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    fn check(&self) -> Result<(), Error> {
        for (name, value) in [
            ("client", self.client.as_str()),
            ("device", self.device.as_str()),
            ("device_id", self.device_id.as_str()),
            ("version", self.version.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(Error::InvalidIdentity(name));
            }
        }
        Ok(())
    }
}

/// `Authorization` header value Jellyfin accepts.
///
/// Order is client, device, device id, version, then token when one is present.
/// Values are percent-encoded the same way as Jellyfin's own clients, so a comma
/// or quote in the device name cannot split the header.
#[must_use]
pub fn authorization_header(identity: &ClientIdentity, token: Option<&str>) -> String {
    let mut header = format!(
        "MediaBrowser Client=\"{}\", Device=\"{}\", DeviceId=\"{}\", Version=\"{}\"",
        encode_component(identity.client()),
        encode_component(identity.device()),
        encode_component(identity.device_id()),
        encode_component(identity.version()),
    );
    if let Some(token) = token.filter(|value| !value.is_empty()) {
        header.push_str(", Token=\"");
        header.push_str(&encode_component(token));
        header.push('"');
    }
    header
}

/// `encodeURIComponent` for header values. Jellyfin URL-decodes each field.
pub(crate) fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for &byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => encoded.push(byte as char),
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 0x0f) as usize] as char);
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> ClientIdentity {
        ClientIdentity::new("DeskQuest", "Quest 3", "device-1", "0.1.0").unwrap()
    }

    #[test]
    fn header_lists_client_device_id_version_and_omits_an_empty_token() {
        let header = authorization_header(&identity(), None);
        assert_eq!(
            header,
            "MediaBrowser Client=\"DeskQuest\", Device=\"Quest%203\", DeviceId=\"device-1\", Version=\"0.1.0\""
        );
        assert!(!header.contains("Token="));
        assert_eq!(authorization_header(&identity(), Some("")), header);
    }

    #[test]
    fn header_appends_the_token_after_the_version() {
        let header = authorization_header(&identity(), Some("test-token"));
        assert_eq!(
            header,
            "MediaBrowser Client=\"DeskQuest\", Device=\"Quest%203\", DeviceId=\"device-1\", Version=\"0.1.0\", Token=\"test-token\""
        );
    }

    #[test]
    fn header_encodes_commas_quotes_and_newlines_inside_one_field() {
        let identity = ClientIdentity::new(
            "DeskQuest",
            "Living Room, \"TV\"\r\nX-Injected: 1",
            "device-1",
            "0.1.0",
        )
        .unwrap();
        let header = authorization_header(&identity, Some("a b"));
        assert_eq!(
            header,
            "MediaBrowser Client=\"DeskQuest\", Device=\"Living%20Room%2C%20%22TV%22%0D%0AX-Injected%3A%201\", DeviceId=\"device-1\", Version=\"0.1.0\", Token=\"a%20b\""
        );
        assert!(!header.contains('\n'));
        assert!(!header.contains('\r'));
    }

    #[test]
    fn encode_component_matches_utf8_percent_encoding() {
        assert_eq!(encode_component("ä"), "%C3%A4");
        assert_eq!(encode_component("A-_.!~*'()"), "A-_.!~*\'()");
    }

    #[test]
    fn empty_identity_fields_are_rejected() {
        assert!(matches!(
            ClientIdentity::new(" ", "Quest 3", "device-1", "0.1.0"),
            Err(Error::InvalidIdentity("client"))
        ));
        assert!(matches!(
            ClientIdentity::new("DeskQuest", "Quest 3", "", "0.1.0"),
            Err(Error::InvalidIdentity("device_id"))
        ));
    }
}
