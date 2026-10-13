// Whether a character is on the network.
//
// Every character starts off the network. `/dn on` puts one character on it, and `/dn off` takes it
// off again. On the network a character is seen online by anyone, told by anyone and listed in the
// network's directory; squelches, the operator's blocks and Appear Offline still have their say. It
// is per character, so a character its player keeps off the network stays private.

namespace Dereth.Network;

public readonly record struct CharacterSettings(bool On)
{
    /// <summary>A character off the network, as every character starts.</summary>
    public static readonly CharacterSettings Off = new(false);

    /// <summary>A character on the network.</summary>
    public static readonly CharacterSettings OnNetwork = new(true);
}
