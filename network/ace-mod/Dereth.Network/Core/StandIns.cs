// Stand-in ids: the object id a world gives each remote correspondent.
//
// A tell, a room line and a friends row each carry a sender id beside the name, and the retail
// client makes a sender's name clickable, and the sender its reply target, only for ids in the
// player range. A remote character has no object on this world, so the world gives it an id of its
// own, from a block no local object uses, and keeps it:
//
// - one id per remote identity (a character's address, or a channel's), the same for every local
//   player, so a reply by id, a stored friend and a squelch all name the same correspondent;
// - persisted, so the id survives a relog, a restart and a lost hub link;
// - never reused, even after the correspondent is forgotten.

namespace Dereth.Network;

public sealed class StandIns
{
    public const uint First = 0x6000_0001;
    public const uint Last = 0x6FFF_FFFF;

    /// <summary>
    /// How far past the next id a saved table says the next id is. A save is written a moment after
    /// the change it records; ids given in that moment, before a crash, are then never given again.
    /// </summary>
    public const uint SaveMargin = 1024;

    private uint next = First;
    private readonly Dictionary<string, uint> byKey = new(StringComparer.Ordinal);
    private readonly SortedDictionary<uint, Address> byId = new();

    /// <summary>Counts every change to the table, so a copy of it can be kept until it changes.</summary>
    public long Version { get; private set; }

    public static bool IsStandIn(uint id) => id is >= First and <= Last;

    /// <summary>The next id to give out.</summary>
    public uint Next => next;

    public int Count => byKey.Count;

    /// <summary>
    /// The id of `who`, given now if it has none; null when the block is used up. The address is
    /// remembered as written, so a later spelling with other capitals updates the display and keeps
    /// the id.
    /// </summary>
    public uint? IdFor(Address who)
    {
        var key = who.Key;
        if (byKey.TryGetValue(key, out var id))
        {
            if (byId[id].ToString() != who.ToString())
            {
                byId[id] = who;
                Version++;
            }
            return id;
        }
        if (next > Last || next < First)
            return null;
        id = next;
        next = id + 1;
        byKey[key] = id;
        byId[id] = who;
        Version++;
        return id;
    }

    /// <summary>
    /// Forgets the identity `id` stands for. The id is never given again: one a client still holds
    /// names no one, never someone else, and the identity gets a new id if it is seen again.
    /// </summary>
    public Address? Forget(uint id)
    {
        if (!byId.Remove(id, out var who))
            return null;
        byKey.Remove(who.Key);
        Version++;
        return who;
    }

    /// <summary>The id `who` already has, if any.</summary>
    public uint? Get(Address who) => byKey.TryGetValue(who.Key, out var id) ? id : null;

    /// <summary>The identity an id stands for, if it is one this world gave.</summary>
    public Address? Who(uint id) => byId.TryGetValue(id, out var a) ? a : null;

    public IEnumerable<KeyValuePair<uint, Address>> All => byId;

    /// <summary>A table as it was saved. Ids already given are kept; the next id is past all of them.</summary>
    public static StandIns Restore(uint savedNext, IEnumerable<KeyValuePair<uint, Address>> given)
    {
        var t = new StandIns { next = savedNext };
        foreach (var (id, who) in given)
        {
            if (!IsStandIn(id))
                continue;
            t.byKey[who.Key] = id;
            t.byId[id] = who;
            if (id >= t.next)
                t.next = id + 1;
        }
        if (t.next < First)
            t.next = First;
        return t;
    }

    /// <summary>The next id as a save records it: moved on by <see cref="SaveMargin"/>, within the block's end.</summary>
    public uint NextForSave()
    {
        var n = (ulong)next + SaveMargin;
        return n > (ulong)Last + 1 ? Last + 1 : (uint)n;
    }

    /// <summary>A copy that shares nothing with this table.</summary>
    public StandIns Copy()
    {
        var t = new StandIns { next = next };
        foreach (var (k, v) in byKey)
            t.byKey[k] = v;
        foreach (var (k, v) in byId)
            t.byId[k] = v;
        return t;
    }

    /// <summary>For tests: a table whose next id is `next`.</summary>
    public static StandIns StartingAt(uint next) => new() { next = next };
}
