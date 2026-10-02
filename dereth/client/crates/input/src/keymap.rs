//! Decoding master keymaps and their input maps, choosing the best binding,
//! comparing binding specificity, and merging maps.
//!
//! The layout follows the client's master-input-map serialiser and its one-input-map
//! serialiser.

use dereth_assets::Decode;

use crate::error::InputError;
use crate::spec::{ControlChord, ControlCode, DeviceType};
use crate::{ActionId, InputMapId};

/// `DeviceKeyMapEntry` — a `DeviceType` plus the DirectInput instance GUID, 17 bytes on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DeviceKeyMapEntry {
    pub device_type: DeviceType,
    pub guid: [u8; 16],
}

/// `GUID_SysKeyboard` -- the client writes this name rather
/// than a literal GUID.
pub const GUID_SYS_KEYBOARD: [u8; 16] = [
    0x61, 0x2B, 0x1D, 0x6F, 0xA0, 0xD5, 0xCF, 0x11, 0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00,
];
/// `GUID_SysMouse`.
pub const GUID_SYS_MOUSE: [u8; 16] = [
    0x60, 0x2B, 0x1D, 0x6F, 0xA0, 0xD5, 0xCF, 0x11, 0xBF, 0xC7, 0x44, 0x45, 0x53, 0x54, 0x00, 0x00,
];
/// `GUID_Virtual` -- the virtual device is added at runtime, so
/// it is not in the shipped tables. Its GUID is all-zero in the client's own construction.
pub const GUID_VIRTUAL: [u8; 16] = [0; 16];

/// One input map's binding list.
///
/// The bindings are kept in **insertion order**, because that is the order the client's intrusive hash
/// list holds them in and therefore the order the serialiser writes them.
#[derive(Debug, Clone, Default)]
pub struct InputMap {
    pub input_map_id: InputMapId,
    bindings: Vec<(ControlChord, ActionId)>,
}

impl InputMap {
    #[must_use]
    pub fn new(id: InputMapId) -> Self {
        Self {
            input_map_id: id,
            bindings: Vec::new(),
        }
    }

    #[must_use]
    pub fn bindings(&self) -> &[(ControlChord, ActionId)] {
        &self.bindings
    }

    /// Bind -- a push to the tail, so a repeat of an existing key
    /// replaces the value in place rather than appending.
    pub fn add_mapping(&mut self, qc: ControlChord, action: ActionId) {
        if let Some(slot) = self.bindings.iter_mut().find(|(k, _)| *k == qc) {
            slot.1 = action;
        } else {
            self.bindings.push((qc, action));
        }
    }

    /// Unbind one control.
    pub fn unbind_by_key(&mut self, qc: &ControlChord) {
        self.bindings.retain(|(k, _)| k != qc);
    }

    /// Unbind every control bound to one action.
    pub fn unbind_all_by_action(&mut self, action: ActionId) {
        self.bindings.retain(|(_, a)| *a != action);
    }

    /// Unbind one unmodified control from one action, whatever its activation.
    pub fn unbind_control_from(&mut self, control: crate::spec::ControlCode, action: ActionId) {
        self.bindings
            .retain(|(k, a)| !(k.control == control && k.meta_mode == 0 && *a == action));
    }

    /// Unbind one unmodified control from whatever it is bound to, whatever its activation.
    pub fn unbind_control(&mut self, control: crate::spec::ControlCode) {
        self.bindings
            .retain(|(k, _)| !(k.control == control && k.meta_mode == 0));
    }

    /// Find the best match for a control.
    ///
    /// Walks the whole list of equal controls and keeps the one the better-match comparison prefers.
    #[must_use]
    pub fn find_best_match(&self, event: &ControlChord) -> Option<(ActionId, ControlChord)> {
        let mut best: Option<(ActionId, ControlChord)> = None;
        for (binding, action) in &self.bindings {
            if !event.matches(binding) {
                continue;
            }
            match &best {
                None => best = Some((*action, *binding)),
                Some((_, b)) if event.is_better_match(binding, b) => {
                    best = Some((*action, *binding))
                }
                Some(_) => {}
            }
        }
        best
    }

    /// Find the conflicting controls -- what the options page uses to list what a rebind would
    /// unbind.
    #[must_use]
    pub fn find_conflicting_controls(&self, qc: &ControlChord) -> Vec<(ControlChord, ActionId)> {
        self.bindings
            .iter()
            .filter(|(k, _)| k.is_conflicting(qc))
            .copied()
            .collect()
    }

    /// Find the keys for an action — every control bound to an action in this map.
    #[must_use]
    pub fn keys_for_action(&self, action: ActionId) -> Vec<ControlChord> {
        self.bindings
            .iter()
            .filter(|(_, a)| *a == action)
            .map(|(k, _)| *k)
            .collect()
    }

    /// Merge -- adds what is not already present. With `overwrite` false an
    /// existing binding for the same `ControlChord` is left alone.
    ///
    /// The client passes `overwrite = true` at all three add-keymap call sites, and the *order* is
    /// what makes a user binding win: the user file is merged first, so by the time a default map
    /// arrives its control is already present and the merge is a no-op for it.
    pub fn merge(&mut self, other: &Self, overwrite: bool) {
        for (qc, action) in &other.bindings {
            match self.bindings.iter_mut().find(|(k, _)| k == qc) {
                Some(slot) => {
                    if overwrite {
                        // Present already: `Merge` adds what is *absent*, so the incoming value is
                        // dropped. `overwrite` selects only whether an equal key is re-pushed.
                        let _ = slot;
                    }
                }
                None => self.bindings.push((*qc, *action)),
            }
        }
    }
}

/// The whole keymap: name, GUID, device table, meta keys and the sections.
#[derive(Debug, Clone)]
pub struct MasterInputMap {
    pub did: u32,
    /// The keymap's name. A freshly constructed map is `"User Defined Keymap"`, which is what the
    /// master map's constructor writes.
    pub name: String,
    pub guid: [u8; 16],
    /// The device table — the index in bits 0–7 of every `ControlCode` is an index into this.
    pub devices: Vec<DeviceKeyMapEntry>,
    /// The meta keys, control → bit. **Data, not constants.**
    pub meta_keys: Vec<(ControlCode, u32)>,
    /// The sections, in file order.
    pub sections: Vec<InputMap>,
    /// The used meta-key bits.
    pub used_meta_keys: u32,
}

impl Default for MasterInputMap {
    fn default() -> Self {
        Self {
            did: 0,
            // The master map's constructor writes this name.
            name: "User Defined Keymap".to_owned(),
            guid: [0; 16],
            devices: Vec::new(),
            meta_keys: Vec::new(),
            sections: Vec::new(),
            used_meta_keys: 0,
        }
    }
}

impl MasterInputMap {
    /// Deserialise a master input map, DID `0x14000000` or `0x14000002`.
    ///
    /// # Errors
    /// The payload must be consumed to its exact end.
    pub fn read(payload: &[u8]) -> Result<Self, InputError> {
        let decoded = dereth_assets::ui::MasterInputMap::decode_bytes(payload)?;

        let mut devices = Vec::with_capacity(decoded.devices.len());
        for (dt, guid) in decoded.devices {
            let device_type = DeviceType::from_raw(dt).ok_or(InputError::Malformed(
                "keymap device table: unknown DeviceType",
            ))?;
            devices.push(DeviceKeyMapEntry { device_type, guid });
        }

        let mut meta_keys = Vec::with_capacity(decoded.meta_keys.len());
        let mut used = 0u32;
        for (cs, mask) in decoded.meta_keys {
            used |= mask;
            meta_keys.push((ControlCode(cs), mask));
        }

        let mut sections = Vec::with_capacity(decoded.input_maps.len());
        for (id, binds) in decoded.input_maps {
            let mut map = InputMap::new(InputMapId(id));
            for b in binds {
                // A `ControlChord` is serialised by the same routine as a control
                // specification -- both are three consecutive dwords, and the linker folded them.
                let qc = ControlChord::new(ControlCode(b.key), b.metamode, b.activation);
                map.bindings.push((qc, ActionId(b.action)));
            }
            sections.push(map);
        }

        Ok(Self {
            did: decoded.id.0,
            name: decoded.name,
            guid: decoded.map_guid,
            devices,
            meta_keys,
            sections,
            used_meta_keys: used,
        })
    }

    #[must_use]
    pub fn section(&self, id: InputMapId) -> Option<&InputMap> {
        self.sections.iter().find(|s| s.input_map_id == id)
    }

    pub fn section_mut(&mut self, id: InputMapId) -> Option<&mut InputMap> {
        self.sections.iter_mut().find(|s| s.input_map_id == id)
    }

    /// Find or create an input map.
    pub fn create_input_map(&mut self, id: InputMapId) -> &mut InputMap {
        let i = match self.sections.iter().position(|s| s.input_map_id == id) {
            Some(i) => i,
            None => {
                self.sections.push(InputMap::new(id));
                self.sections.len() - 1
            }
        };
        &mut self.sections[i]
    }

    /// Add a device -- returns the index, which becomes bits 0-7 of
    /// every `ControlCode` for that device. An existing GUID keeps its index, which is why
    /// device *ordering* has to be reproduced (a saved keymap serialises the index).
    pub fn add_device_entry(&mut self, device_type: DeviceType, guid: [u8; 16]) -> u8 {
        if let Some(i) = self.devices.iter().position(|d| d.guid == guid) {
            // The table is indexed by a u8 field; a keymap can never hold more than 256 devices.
            #[allow(clippy::cast_possible_truncation)]
            return i as u8;
        }
        self.devices.push(DeviceKeyMapEntry { device_type, guid });
        #[allow(clippy::cast_possible_truncation)]
        {
            (self.devices.len() - 1) as u8
        }
    }

    /// The `DeviceType` a control's device index selects. Used everywhere the barrier maps need to
    /// know "is this the keyboard".
    #[must_use]
    pub fn device_type_of(&self, cs: ControlCode) -> Option<DeviceType> {
        self.devices
            .get(cs.device_index() as usize)
            .map(|d| d.device_type)
    }

    /// Whether a control is a meta key.
    #[must_use]
    pub fn is_meta_key(&self, cs: ControlCode) -> bool {
        self.meta_keys.iter().any(|(k, _)| *k == cs)
    }

    /// The meta mode a control selects -- 0 when the control is not a meta key.
    #[must_use]
    pub fn meta_mode_from_key(&self, cs: ControlCode) -> u32 {
        self.meta_keys
            .iter()
            .find(|(k, _)| *k == cs)
            .map_or(0, |(_, m)| *m)
    }

    /// The control a meta mode came from.
    #[must_use]
    pub fn key_from_meta_mode(&self, mask: u32) -> Option<ControlCode> {
        self.meta_keys
            .iter()
            .find(|(_, m)| *m == mask)
            .map(|(k, _)| *k)
    }

    /// Clear -- devices, meta keys and every section.
    ///
    /// **Adds what is not already present.** The client's load order is the user file, then
    /// the game default map, then `DefaultMap`, so a default binding is only applied to a control the
    /// user file did not mention.
    pub fn merge(&mut self, other: &Self, overwrite: bool) {
        for d in &other.devices {
            self.add_device_entry(d.device_type, d.guid);
        }
        for (cs, mask) in &other.meta_keys {
            if !self.meta_keys.iter().any(|(k, _)| k == cs) {
                self.meta_keys.push((*cs, *mask));
                self.used_meta_keys |= *mask;
            }
        }
        for sec in &other.sections {
            self.create_input_map(sec.input_map_id)
                .merge(sec, overwrite);
        }
    }

    /// Clear the whole master map.
    pub fn clear(&mut self) {
        self.devices.clear();
        self.meta_keys.clear();
        self.sections.clear();
        self.used_meta_keys = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A master input map payload: `did`, the name's packed length and bytes, a zero GUID, one
    /// keyboard device, no meta keys and one section holding one binding.
    fn payload(name_len: &[u8], name: &[u8]) -> Vec<u8> {
        let mut p = 0x1400_0002u32.to_le_bytes().to_vec();
        p.extend_from_slice(name_len);
        p.extend_from_slice(name);
        p.extend_from_slice(&[0u8; 16]);
        p.extend_from_slice(&1u32.to_le_bytes());
        p.push(1);
        p.extend_from_slice(&GUID_SYS_KEYBOARD);
        p.extend_from_slice(&0u32.to_le_bytes());
        p.extend_from_slice(&1u32.to_le_bytes());
        p.extend_from_slice(&3u32.to_le_bytes());
        p.extend_from_slice(&1u32.to_le_bytes());
        for v in [0x0011_0000u32, 0, 1, 0x6000_0005] {
            p.extend_from_slice(&v.to_le_bytes());
        }
        p
    }

    /// The keymap's name is a narrow archive string in the client's code page, Windows-1252: the
    /// bytes `0x80` and `0x91` are the euro sign and the left single quote, not the control
    /// characters U+0080 and U+0091 a byte-for-code-point reading would give.
    #[test]
    fn a_keymap_name_reads_its_high_bytes_as_windows_1252() {
        let m = MasterInputMap::read(&payload(&[4], &[b'A', 0x80, 0x91, b'z'])).expect("decodes");
        assert_eq!(m.name, "A\u{20AC}\u{2018}z");
        assert_eq!(m.did, 0x1400_0002);
        assert_eq!(m.devices.len(), 1);
        assert_eq!(m.devices[0].device_type, DeviceType::Keyboard);
        let s = m.section(InputMapId(3)).expect("section 3");
        assert_eq!(s.bindings().len(), 1);
        assert_eq!(s.bindings()[0].1, ActionId(0x6000_0005));
    }

    /// A name whose packed length runs past the payload, or a payload with a byte left over, is
    /// an error and never a short read; an unknown device type is refused. The two-byte packed
    /// form `80 04` is 4.
    #[test]
    fn a_malformed_name_length_or_a_leftover_byte_is_refused() {
        assert!(MasterInputMap::read(&payload(&[0x80, 0x04], b"Keys")).is_ok());
        let long = payload(&[0x7F], b"Keys");
        assert!(matches!(
            MasterInputMap::read(&long),
            Err(InputError::Decode(_))
        ));
        let mut extra = payload(&[4], b"Keys");
        extra.push(0);
        assert!(matches!(
            MasterInputMap::read(&extra),
            Err(InputError::Decode(_))
        ));
        let mut bad_device = payload(&[4], b"Keys");
        bad_device[4 + 1 + 4 + 16 + 4] = 9;
        assert!(matches!(
            MasterInputMap::read(&bad_device),
            Err(InputError::Malformed(_))
        ));
    }
}
