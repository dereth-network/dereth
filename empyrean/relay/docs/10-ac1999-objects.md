# ac1999 identity and object creation

After the typed selection exchange, ac1999 queue 3 can establish the selected
player's identity (`F746`) and deliver object-create descriptors (`F745`). The
relay writes these again as the later messages. A second `F745` for a known object is this wire's
update, and reaches Dereth as `F7DB`.

## Wire admission

Identity is exactly one u32 id. It must follow the selection send and match the
chosen character. An unsolicited identity is ignored, the same identity is
published only once, and a different identity closes entry. The queue is checked
before decoding. A matching id publishes the existing `PlayerCreated` event;
it does not create a stand-in physics object.

The dedicated `ac1999::objects::CreateObject` codec consumes the whole message:

1. Object id.
2. Appearance: version `11`, three byte counts, optional palette id and arrays,
   then absolute four-byte alignment. Asset offsets are **fixed u16**, including
   values with the high bit set. They are not later compressed ids.
3. Physics descriptor, read as the November 1999 client reads it, with nine u16 timestamps.
4. Public object descriptor with ac1999 strings and field widths.

Appearance arrays use four bytes per palette replacement, five per texture
replacement and three per model replacement. The palette, texture and model
type prefixes are restored in the normalized model. The public class and icon
offset are fixed u16; ammunition is u8. A zero-length name consumes two bytes
without padding, and the fixed public fields immediately follow it. Optional
target type precedes effects; radar precedes the vendor/spell words.

The early public-header bit `00200000` carries a trade note's issuing vendor
class. The codec preserves it separately as `vendor_class`. Normalization clears
that header bit and leaves later `burden` absent. The world model does not yet
consume vendor-class metadata, and ac1999 vendor interaction remains unsupported.

The physics descriptor has the later order and widths without translucency: the movement buffer
(its length, the bytes and an autonomy word, or only a zero length) or else the animation frame,
the position, the motion, sound, physics-script and setup ids, the parent, the child links (a
count and id/location pairs, at most 256 as policy), scale, friction, elasticity, velocity,
acceleration, omega, the default script and its intensity, then the timestamps and alignment. Bits
the client has no field for (translucency, the timestamp flag, anything above them) are dropped,
and an animation frame is not read beside a movement buffer, so the normalized descriptor holds
nothing the later codec could misread. Alignment padding is skipped unchecked, as the client skips
it.

Admission rejects zero object ids, unsupported public or classification bits, truncated or
trailing data, invalid string
forms, nonfinite numeric geometry and zero quaternions. Names use the existing
255-byte ac1999 limit; appearance counts are bounded by their byte widths.
Encoding refuses lossy asset/class/ammunition widths and mismatched optional
fields. The supported normalized model must round-trip through the canonical
object codec before it is admitted.

## Proof boundary

Independent literal vectors exercise high-bit fixed asset ids, all admitted
public fields, empty-name alignment, nine timestamps and the vendor/burden
distinction. Prefix truncation and malformed field tests cover both read and
write rejection. In-process peer scenarios run the real handshake, selection,
typed object boundary, host object phase and shared world pipeline. They cover
matching identity, duplicates, mismatches, queued parents, stale instances,
wrong queues, malformed creates and disconnect cancellation.

World tests supply explicit synthetic setup facts. They do not establish classic
DAT closure, valid render assets or an operational scene against an upstream
server. Appearance updates, removals and position updates now follow the
[world stream](11-ac1999-world.md); the player description (`0013`), qualities and
gameplay remain outstanding.
