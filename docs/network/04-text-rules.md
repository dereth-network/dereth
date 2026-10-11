# The text rules

Provenance: **network specification**; the client behaviour each rule answers is **observed at
runtime** in the final retail client (June 2015).

One implementation: `network/protocol/src/text.rs`. The vectors in
`network/conformance/vectors/text.json` pin each rule.

**A receiving world never trusts the sending world.** Every remote line — a tell, a room line, a
channel line, an away message — and every remote sender's name pass these rules before any client
sees them, whatever the sending world claims to have done. Dereth and the web client are Unicode;
the rules exist for the oldest client that may read the line.

## Why

The final retail client:

- draws chat in Windows-1252. A tell's text and a system line travel as Windows-1252 bytes; a
  TurbineChat room line travels as UTF-16 and each of its fields is narrowed to Windows-1252 and
  **ends at the first character the code page cannot hold**, so one emoji cuts the rest of the
  line;
- reads `<Tell:IIDString:id:name>name<\Tell>` runs as clickable name links in **every** line it
  draws, server text included, and fills `@tell name, ` from one when clicked, so a sender could
  forge a link to anyone;
- **drops** a room line that holds `<tell:` in any case, a line feed, or 256 or more characters;
- breaks a tell at a line feed and ends one at a NUL;
- takes a leading `+` on a name as the administrator marker, and reads `^` or `&` anywhere in a
  speaker's name as the Olthoi speech marks (and garbles the line for a listener who is not one).

## The rules, in order

For a line:

1. **Markup out.** Every `<tell:` or `<tell ` opening, up to and including its `>`, and every
   `<\tell>` or `</tell>` closing, matched without regard to case, is removed. An opening with no
   `>` loses its `<`.
2. **Line breaks and controls.** A carriage return, line feed, tab, U+0085, U+2028 or U+2029 becomes
   one space. Every other control character (U+0000–U+001F, U+007F–U+009F) is removed.
3. **Windows-1252.** Every character becomes one Windows-1252 holds: itself if it has a byte, else
   its Windows best fit (`ā` → `a`, fullwidth `＜` → `<`), else `?`. The best fits are Microsoft's
   published table for code page 1252.
4. **Markup again**, until none is left: a best fit can make a bracket.
5. **Trim** white space at both ends.
6. **Cut** at 255 characters, then trim white space at the end again (a cut just after a space
   leaves one).

For a sender's name: the line rules, then every leading `+` is removed, and any white space before
or between them; a name holding `^` or `&` **anywhere** is **refused** (the line is not shown), as
is one that is empty. A conformant address cannot hold either mark at all
([01-addresses-and-settings.md](01-addresses-and-settings.md)); a world that sends one is broken or
impersonating Olthoi speech.

The same rules apply to anything else another world or the hub names that a world shows its
players: a world's name in `/dn worlds`, and the away text of a tell's answer (the sending world
writes the *... is away:* line around it itself).

The rules are idempotent: a line or a name that has passed them passes them unchanged.
