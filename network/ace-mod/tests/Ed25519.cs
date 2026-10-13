// Ed25519 signing (RFC 8032 section 5.1), for the harness alone: it signs the interactions it hands a
// real hub as Discord signs them. .NET has no Ed25519 of its own, so this is the RFC's reference
// arithmetic over System.Numerics.BigInteger: slow, plain, and never used by the mod.

using System.Numerics;
using System.Security.Cryptography;

namespace Dereth.Network.Harness;

internal static class Ed25519
{
    private static readonly BigInteger P = BigInteger.Pow(2, 255) - 19;
    private static readonly BigInteger L = BigInteger.Pow(2, 252) + BigInteger.Parse("27742317777372353535851937790883648493");
    private static readonly BigInteger D = Mod(-121665 * Inverse(121666));
    private static readonly BigInteger SqrtMinusOne = BigInteger.ModPow(2, (P - 1) / 4, P);
    private static readonly (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) Base = BasePoint();

    private static BigInteger Mod(BigInteger a)
    {
        var r = a % P;
        return r.Sign < 0 ? r + P : r;
    }

    private static BigInteger Inverse(BigInteger x) => BigInteger.ModPow(Mod(x), P - 2, P);

    private static (BigInteger, BigInteger, BigInteger, BigInteger) BasePoint()
    {
        var y = Mod(4 * Inverse(5));
        var x = RecoverX(y, 0);
        return (x, y, 1, Mod(x * y));
    }

    private static BigInteger RecoverX(BigInteger y, int sign)
    {
        var x2 = Mod((y * y - 1) * Inverse(D * y * y + 1));
        var x = BigInteger.ModPow(x2, (P + 3) / 8, P);
        if (Mod(x * x - x2) != 0)
            x = Mod(x * SqrtMinusOne);
        if ((int)(x % 2) != sign)
            x = P - x;
        return x;
    }

    private static (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) Add(
        (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) p, (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) q)
    {
        var a = Mod((p.Y - p.X) * (q.Y - q.X));
        var b = Mod((p.Y + p.X) * (q.Y + q.X));
        var c = Mod(2 * p.T * q.T * D);
        var d = Mod(2 * p.Z * q.Z);
        var (e, f, g, h) = (Mod(b - a), Mod(d - c), Mod(d + c), Mod(b + a));
        return (Mod(e * f), Mod(g * h), Mod(f * g), Mod(e * h));
    }

    private static (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) Times(BigInteger s, (BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) p)
    {
        (BigInteger, BigInteger, BigInteger, BigInteger) q = (0, 1, 1, 0);
        while (s > 0)
        {
            if (!s.IsEven)
                q = Add(q, p);
            p = Add(p, p);
            s >>= 1;
        }
        return q;
    }

    private static byte[] Encode((BigInteger X, BigInteger Y, BigInteger Z, BigInteger T) p)
    {
        var zi = Inverse(p.Z);
        var x = Mod(p.X * zi);
        var y = Mod(p.Y * zi);
        var bytes = Little(y);
        bytes[31] |= (byte)((int)(x % 2) << 7);
        return bytes;
    }

    private static byte[] Little(BigInteger n)
    {
        var raw = n.ToByteArray(isUnsigned: true, isBigEndian: false);
        var out32 = new byte[32];
        Array.Copy(raw, out32, Math.Min(32, raw.Length));
        return out32;
    }

    private static BigInteger Number(ReadOnlySpan<byte> bytes) => new(bytes, isUnsigned: true, isBigEndian: false);

    /// <summary>The secret key's scalar and the prefix its signatures are made with.</summary>
    private static (BigInteger A, byte[] Prefix) Expand(byte[] secret)
    {
        var h = SHA512.HashData(secret);
        var a = h[..32];
        a[0] &= 248;
        a[31] &= 127;
        a[31] |= 64;
        return (Number(a), h[32..]);
    }

    /// <summary>The public key of a 32-byte secret key.</summary>
    public static byte[] PublicKey(byte[] secret) => Encode(Times(Expand(secret).A, Base));

    /// <summary>The 64-byte signature of `message` under a 32-byte secret key.</summary>
    public static byte[] Sign(byte[] secret, byte[] message)
    {
        var (a, prefix) = Expand(secret);
        var publicKey = Encode(Times(a, Base));
        var r = Number(SHA512.HashData([.. prefix, .. message])) % L;
        var rEncoded = Encode(Times(r, Base));
        var k = Number(SHA512.HashData([.. rEncoded, .. publicKey, .. message])) % L;
        var s = (r + k * a) % L;
        return [.. rEncoded, .. Little(s)];
    }
}
