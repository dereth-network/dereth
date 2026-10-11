// A character's network settings.
//
// Every character starts off the network. `/dn on` opts one character in, with the cautious
// settings (mutual friends only); each setting can then be widened or narrowed, and `/dn off`
// takes the character off again. Settings are per character, so an alt or a mule stays hidden
// unless its owner opts it in too.

namespace Dereth.Network;

/// <summary>Who sees a character's online status.</summary>
public enum Visible
{
    /// <summary>Nobody: the character always shows offline.</summary>
    Nobody,
    /// <summary>Mutual friends only: characters it lists that list it too.</summary>
    Friends,
    /// <summary>Mutual friends and the members of its channels.</summary>
    Channels,
    /// <summary>Anyone who lists it.</summary>
    Anyone,
}

/// <summary>
/// Who may send a character a first tell. A character can always be answered by anyone it told
/// within the reply window, so a conversation it starts is never one-sided.
/// </summary>
public enum Reachable
{
    Nobody,
    Friends,
    Anyone,
}

/// <summary>Whether a character's profile is published in the hub's directory.</summary>
public enum Listed
{
    Off,
    On,
}

public readonly record struct CharacterSettings(bool On, Visible Visible, Reachable Reachable, Listed Listed)
{
    /// <summary>A character that never opted in: off the network, every setting off.</summary>
    public static readonly CharacterSettings Off = new(false, Visible.Nobody, Reachable.Nobody, Listed.Off);

    /// <summary>What `/dn on` gives a character the first time: seen by mutual friends, reachable by anyone, not listed.</summary>
    public static readonly CharacterSettings OptedIn = new(true, Visible.Friends, Reachable.Anyone, Listed.Off);
}

/// <summary>The words `/dn` and the saved records spell the settings with.</summary>
public static class Words
{
    public static string Of(Visible v) => v switch
    {
        Visible.Nobody => "nobody",
        Visible.Friends => "friends",
        Visible.Channels => "channels",
        _ => "anyone",
    };

    public static string Of(Reachable r) => r switch
    {
        Reachable.Nobody => "nobody",
        Reachable.Friends => "friends",
        _ => "anyone",
    };

    public static string Of(Listed l) => l == Listed.On ? "on" : "off";

    public static bool TryVisible(string s, out Visible v)
    {
        switch (Text.AsciiLower(s.Trim()))
        {
            case "nobody": v = Visible.Nobody; return true;
            case "friends": v = Visible.Friends; return true;
            case "channels": v = Visible.Channels; return true;
            case "anyone": v = Visible.Anyone; return true;
            default: v = default; return false;
        }
    }

    public static bool TryReachable(string s, out Reachable r)
    {
        switch (Text.AsciiLower(s.Trim()))
        {
            case "nobody": r = Reachable.Nobody; return true;
            case "friends": r = Reachable.Friends; return true;
            case "anyone": r = Reachable.Anyone; return true;
            default: r = default; return false;
        }
    }

    public static bool TryListed(string s, out Listed l)
    {
        switch (Text.AsciiLower(s.Trim()))
        {
            case "off": l = Listed.Off; return true;
            case "on": l = Listed.On; return true;
            default: l = default; return false;
        }
    }
}
