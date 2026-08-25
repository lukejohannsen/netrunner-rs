use serde::{Deserialize, Serialize};

/// Raw wire shape of a pack (set) object from NetrunnerDB's
/// `/api/2.0/public/packs` endpoint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetrunnerDbPackDto {
    pub code: String,
    pub name: String,
    #[serde(default)]
    pub cycle_code: Option<String>,
    #[serde(default)]
    pub position: Option<u32>,
}

/// Trimmed, always-valid pack summary — packs have no faction/type codes to
/// validate, so this conversion is infallible (`From`, not `TryFrom`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackInfo {
    pub code: String,
    pub name: String,
}

impl From<NetrunnerDbPackDto> for PackInfo {
    fn from(dto: NetrunnerDbPackDto) -> Self {
        Self { code: dto.code, name: dto.name }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_pack_dto_to_pack_info() {
        let dto = NetrunnerDbPackDto {
            code: "sg".to_string(),
            name: "System Gateway".to_string(),
            cycle_code: Some("system-gateway".to_string()),
            position: Some(1),
        };

        let info: PackInfo = dto.into();
        assert_eq!(info.code, "sg");
        assert_eq!(info.name, "System Gateway");
    }
}
