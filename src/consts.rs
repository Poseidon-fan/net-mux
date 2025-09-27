pub(crate) type Version = u8;
pub(crate) type SessionMode = bool;

pub(crate) const PROTOCOL_V0: Version = 0x0;
pub(crate) const SERVER_MODE: SessionMode = false;
pub(crate) const CLIENT_MODE: SessionMode = true;
