// The few pieces of the game's own message layout the mod reads and writes itself: a
// length-prefixed Windows-1252 string, the rows of `Social_FriendsUpdate` (`0x0021`), and the
// character screen's message.

namespace Dereth.Network;

public static class Payload
{
    /// <summary>The opcode a game event travels under.</summary>
    public const uint GameEventOpcode = 0xF7B0;

    /// <summary>`Social_FriendsUpdate`'s event type.</summary>
    public const uint FriendsUpdateEvent = 0x0021;

    /// <summary>
    /// A string as the client writes it: a 16-bit length, that many Windows-1252 bytes, and padding
    /// to a multiple of four bytes counting the length.
    /// </summary>
    public static string ReadString16L(BinaryReader r)
    {
        var len = r.ReadUInt16();
        var bytes = r.ReadBytes(len);
        if (bytes.Length != len)
            throw new EndOfStreamException();
        var pad = (4 - (2 + len) % 4) % 4;
        r.BaseStream.Position += pad;
        return TextRules.DecodeCp1252(bytes);
    }

    /// <summary>A string as <see cref="ReadString16L"/> reads it.</summary>
    public static void WriteString16L(BinaryWriter w, string s)
    {
        var bytes = TextRules.EncodeCp1252(s);
        w.Write((ushort)bytes.Length);
        w.Write(bytes);
        var pad = (4 - (2 + bytes.Length) % 4) % 4;
        for (var i = 0; i < pad; i++)
            w.Write((byte)0);
    }

    /// <summary>`Login_CharacterScreenMessage`'s opcode: the world's message on the character screen.</summary>
    public const uint CharacterScreenMessageOpcode = 0xF65A;

    /// <summary>
    /// `Login_CharacterScreenMessage`'s body: the message, then a second string the client shows
    /// after it, left empty.
    /// </summary>
    public static void WriteCharacterScreenMessage(BinaryWriter w, string text)
    {
        WriteString16L(w, text);
        WriteString16L(w, "");
    }

    /// <summary>`Communication_TurbineChat`'s opcode.</summary>
    public const uint TurbineChatOpcode = 0xF7DE;

    /// <summary>
    /// A room line as the client sends it (`Communication_TurbineChat`, a request to the room by its
    /// id), opcode first. It carries context 0, which the client never gives a request of its own,
    /// so the answer the room sends for it matches nothing the client is waiting on.
    /// </summary>
    public static byte[] TurbineRoomRequest(uint room, uint chatType, uint sender, string text)
    {
        var units = text.Length > 0x3FFF ? text[..0x3FFF] : text;
        using var body = new MemoryStream();
        using (var b = new BinaryWriter(body, System.Text.Encoding.Unicode, leaveOpen: true))
        {
            b.Write(0u); // context
            b.Write(2u); // the request's response type
            b.Write(2u); // send to a room by its id
            b.Write(room);
            if (units.Length < 0x80)
            {
                b.Write((byte)units.Length);
            }
            else
            {
                b.Write((byte)(0x80 | (units.Length >> 8)));
                b.Write((byte)(units.Length & 0xFF));
            }
            b.Write(System.Text.Encoding.Unicode.GetBytes(units));
            b.Write(12u);
            b.Write(sender);
            b.Write(0u);
            b.Write(chatType);
        }
        var bodyBytes = body.ToArray();
        using var packet = new MemoryStream();
        using (var p = new BinaryWriter(packet, System.Text.Encoding.UTF8, leaveOpen: true))
        {
            foreach (var word in new uint[] { 3, 2, 1, 0, 0, 0, 0 })
                p.Write(word);
            p.Write((uint)bodyBytes.Length);
            p.Write(bodyBytes);
        }
        var packetBytes = packet.ToArray();
        using var message = new MemoryStream();
        using (var m = new BinaryWriter(message, System.Text.Encoding.UTF8, leaveOpen: true))
        {
            m.Write(TurbineChatOpcode);
            m.Write((uint)packetBytes.Length);
            m.Write(packetBytes);
        }
        return message.ToArray();
    }

    /// <summary>One friends-list row: id, online, appear-offline (always 0), name, and two empty id lists.</summary>
    public static void WriteFriendRow(BinaryWriter w, uint id, string name, bool online)
    {
        w.Write(id);
        w.Write(online ? 1u : 0u);
        w.Write(0u);
        WriteString16L(w, name);
        w.Write(0u);
        w.Write(0u);
    }

    /// <summary>The character option a client toggles to appear offline.</summary>
    public const uint AppearOfflineOption = 0x27;

    /// <summary>
    /// Whether `data` is a full friends list (the event's header, the row count, the rows, and the
    /// update type 0), and the character it is for (the event's header names it).
    /// </summary>
    public static bool IsFullFriendsList(MemoryStream data, out uint character)
    {
        character = 0;
        if (data.Length < 24)
            return false;
        var end = data.Position;
        try
        {
            var head = new byte[16];
            var tail = new byte[4];
            data.Position = 0;
            if (data.Read(head, 0, 16) != 16)
                return false;
            data.Position = data.Length - 4;
            if (data.Read(tail, 0, 4) != 4)
                return false;
            if (BitConverter.ToUInt32(head, 0) != GameEventOpcode || BitConverter.ToUInt32(head, 12) != FriendsUpdateEvent
                || BitConverter.ToUInt32(tail, 0) != 0)
                return false;
            character = BitConverter.ToUInt32(head, 4);
            return true;
        }
        finally
        {
            data.Position = end;
        }
    }

    /// <summary>
    /// Adds `rows` after the rows of a full friends list already written to `data` (the event's
    /// header, the row count, the rows, and the update type 0), fixing the count. False, with `data`
    /// unchanged, when `data` is not a full friends list.
    /// </summary>
    public static bool AppendFriendRows(MemoryStream data, IReadOnlyList<(uint Id, string Name, bool Online)> rows)
    {
        if (rows.Count == 0)
            return true;
        if (data.Length < 24)
            return false;
        var head = new byte[20];
        data.Position = 0;
        if (data.Read(head, 0, 20) != 20)
            return false;
        var opcode = BitConverter.ToUInt32(head, 0);
        var eventType = BitConverter.ToUInt32(head, 12);
        var count = BitConverter.ToUInt32(head, 16);
        var tail = new byte[4];
        data.Position = data.Length - 4;
        if (data.Read(tail, 0, 4) != 4)
            return false;
        var updateType = BitConverter.ToUInt32(tail, 0);
        if (opcode != GameEventOpcode || eventType != FriendsUpdateEvent || updateType != 0)
        {
            data.Position = data.Length;
            return false;
        }
        using var w = new BinaryWriter(data, System.Text.Encoding.UTF8, leaveOpen: true);
        data.Position = data.Length - 4;
        foreach (var (id, name, online) in rows)
            WriteFriendRow(w, id, name, online);
        w.Write(0u);
        var end = data.Position;
        data.Position = 16;
        w.Write(count + (uint)rows.Count);
        w.Flush();
        data.Position = end;
        return true;
    }
}
