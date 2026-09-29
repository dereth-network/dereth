// Driver for tests/fixtures/ace_common.tsv (empyrean-common, unit 0.3).
// Compiled together with ACE's own ACE.Common sources (Cryptography/ISAAC.cs, CryptoSystem.cs,
// Hash32.cs without the Span overload, DerethDateTime.cs with the UtcNow properties replaced by
// EmuAt/GdleAt(DateTime), Extensions/*.cs, Time.cs) and ace_oracle_shims.cs, then run on the
// .NET 8.0.22 runtime as dotnet_oracle.cs describes. Values that depend on float-to-int casts
// (FloatExtensions.Round of 1e10/NaN, TimeSpanExtensions.GetMonths of a negative span) are .NET 8
// results; net10.0 saturates them, and the Rust tests assert the net10.0 values.
using System;
using System.Globalization;
using System.Collections.Generic;
using System.Text;
using ACE.Common;
using ACE.Common.Cryptography;
using ACE.Common.Extensions;
class P {
  static StringBuilder sb = new StringBuilder();
  static void L(params string[] f){ sb.Append(string.Join("\t", f)).Append('\n'); }
  static string DB(double d){ return "d:" + BitConverter.DoubleToInt64Bits(d).ToString("X16"); }
  static string FB(float f){ return "f:" + BitConverter.ToInt32(BitConverter.GetBytes(f), 0).ToString("X8"); }
  static string DD(DerethDateTime d){ return d.Ticks.ToString("R") + "|" + d.Year + "|" + d.Month + "|" + d.Day + "|" + d.Hour + "|" + d.ToString() + "|" + d.Season + "|" + d.IsDaytime; }
  static void Main() {
    System.Threading.Thread.CurrentThread.CurrentCulture = new CultureInfo("en-US");
    foreach (var seed in new uint[]{0, 1, 0x12345678, 0xFFFFFFFF, 0xDEADBEEF}) {
      var c = new CryptoSystem(seed); var a = new List<string>(); a.Add(c.CurrentKey.ToString("X8"));
      for (int i = 0; i < 600; i++) { var v = c.Next(); if (i < 12 || i % 97 == 0 || i > 590) a.Add(i + ":" + v.ToString("X8")); }
      L("isaac", seed.ToString("X8"), string.Join(",", a.ToArray()));
    }
    { var c = new CryptoSystem(0x12345678u); var ref0 = new CryptoSystem(0x12345678u); var keys = new List<uint>(); keys.Add(ref0.CurrentKey); for (int i = 0; i < 400; i++) keys.Add(ref0.Next());
      var log = new List<string>();
      Func<uint, string> S = x => { bool r = c.Search(x); return (r ? "T" : "F") + c.xors.Count + ":" + c.CurrentKey.ToString("X8"); };
      log.Add(S(keys[0])); c.ConsumeKey(keys[0]); log.Add(c.CurrentKey.ToString("X8"));
      log.Add(S(keys[3])); log.Add(S(keys[1])); c.ConsumeKey(keys[3]); log.Add(c.xors.Count + ":" + c.CurrentKey.ToString("X8"));
      c.ConsumeKey(keys[1]); log.Add(c.xors.Count + ":" + c.CurrentKey.ToString("X8"));
      log.Add(S(0xABCDEF01)); log.Add(S(keys[300]));
      var hs = new List<string>(); foreach (var x in c.xors) hs.Add(x.ToString("X8")); log.Add(hs.Count > 5 ? string.Join(",", hs.GetRange(0,5).ToArray()) : string.Join(",", hs.ToArray()));
      L("crypto_search", string.Join("|", log.ToArray()));
    }
    { var bufs = new List<byte[]>{ new byte[0], new byte[]{1}, new byte[]{1,2,3}, new byte[]{1,2,3,4}, new byte[]{0xFF,0xFF,0xFF,0xFF,0xFF,0xFF,0xFF}, new byte[]{0x10,0x20,0x30,0x40,0x50,0x60,0x70,0x80,0x90,0xA0,0xB0} };
      var rb = new byte[100]; new Random(7).NextBytes(rb); bufs.Add(rb);
      foreach (var b in bufs) { var hx = BitConverter.ToString(b).Replace("-",""); L("hash32", hx, Hash32.Calculate(b, 0, b.Length).ToString("X8")); }
      L("hash32_off", "rb[3..50]", Hash32.Calculate(rb, 3, 47).ToString("X8"));
    }
    foreach (var t in new double[]{0, 1, 100, 209, 210, 238, 476.25, 1000, 7620, 3810, 7619.99, 12345.678, 1e6, 3e8, 3.2e8 + 0.125, 1073727423, 1073727424, 1073727000, 685000, 2286000, 2743200 - 1}) {
      L("ddt_ticks", DB(t), DD(new DerethDateTime(t)));
    }
    foreach (var y in new[]{10, 11, 150, 401}) foreach (var m in new[]{-2,-1,0,1,5,9}) foreach (var d in new[]{1, 2, 30}) foreach (var h in new[]{1, 8, 9, 10, 16}) {
      try { var x = new DerethDateTime(y, m, d, h); L("ddt_ymdh", y + "," + m + "," + d + "," + h, DD(x)); } catch (Exception e) { L("ddt_ymdh", y + "," + m + "," + d + "," + h, "EX:" + e.GetType().Name); }
    }
    { var b = new DerethDateTime(150, 9, 30, 16);
      foreach (var n in new[]{0, 1, 2, 13, -1, -40, 400}) {
        try { L("ddt_addh", n.ToString(), DD(b.AddHours(n))); } catch (Exception e) { L("ddt_addh", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_addd", n.ToString(), DD(b.AddDays(n))); } catch (Exception e) { L("ddt_addd", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_addm", n.ToString(), DD(b.AddMonths(n))); } catch (Exception e) { L("ddt_addm", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_addy", n.ToString(), DD(b.AddYears(n))); } catch (Exception e) { L("ddt_addy", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_addt", n.ToString(), DD(b.AddTicks(n * 1000.5))); } catch (Exception e) { L("ddt_addt", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_subd", n.ToString(), DD(b.SubtractDays(n))); } catch (Exception e) { L("ddt_subd", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_subh", n.ToString(), DD(b.SubtractHours(n))); } catch (Exception e) { L("ddt_subh", n.ToString(), "EX:" + e.GetType().Name); }
        try { L("ddt_subm", n.ToString(), DD(b.SubtractMonths(n))); } catch (Exception e) { L("ddt_subm", n.ToString(), "EX:" + e.GetType().Name); }
      }
    }
    foreach (var dt in new[]{ new DateTime(2024,1,5,0,7,9), new DateTime(2026,9,22,13,45,0), new DateTime(2017,1,31,17,0,0), new DateTime(2000,3,31,1,29,0), new DateTime(2000,12,31,23,59,0), new DateTime(2020,5,5,10,30,0)}) {
      try { L("ddt_emu", dt.Ticks.ToString(), DD(DerethDateTime.EmuAt(dt))); } catch (Exception e) { L("ddt_emu", dt.Ticks.ToString(), "EX:" + e.GetType().Name); }
      try { L("ddt_gdle", dt.Ticks.ToString(), DD(DerethDateTime.GdleAt(dt))); } catch (Exception e) { L("ddt_gdle", dt.Ticks.ToString(), "EX:" + e.GetType().Name); }
      try { L("ddt_lore", dt.Ticks.ToString(), DD(DerethDateTime.ConvertRealWorldToLoreDateTime(dt))); } catch (Exception e) { L("ddt_lore", dt.Ticks.ToString(), "EX:" + e.GetType().Name); }
    }
    foreach (var c in new double[]{1, 0, 0.5, 0.123456, 0.0001234, 1e-9, 0.99999, 0.1, 0.25, 1.5, 0.000000000000000000001}) L("formatchance", DB(c), c.FormatChance());
    foreach (var f in new float[]{2.5f, -2.5f, 1.45f, 2.675f, 1e10f, float.NaN, 0.5f, -0.5f, 1.5f}) foreach (var dp in new[]{0, 1, 2}) L("roundf", FB(f), dp.ToString(), f.Round(dp).ToString(), FB(f.Truncate(dp)));
    foreach (var f in new double[]{2.5, -2.5, 1.45, 2.675, 1e10, double.NaN, 0.5}) foreach (var dp in new[]{0, 1, 2}) L("roundd", DB(f), dp.ToString(), f.Round(dp).ToString());
    L("eps", (1.0f).EpsilonEquals(1.00005f).ToString(), (1.0f).EpsilonEquals(1.0002f).ToString(), (float.NaN).EpsilonEquals(float.NaN).ToString());
    foreach (var s in new[]{"Sarcophagus", "Torch", "Glass", "Dish", "Box", "Topaz", "Moth", "Drudge", "Ch", "s", "", "Us"}) L("plural", s, s.Pluralize());
    foreach (var s in new[]{"apple", "Egg", "banana", "", "i", "Y"}) L("vowel", s, s.StartsWithVowel().ToString());
    L("trimstart", "Hello World".TrimStart("hello "), "Hello".TrimStart("x"), "abcabc".TrimEnd("ABC"), "abc".TrimEnd("abcd"));
    foreach (var s in new[]{"a*b", "*.txt", "x+y?(z)[w]{1}^$|#\\ .", "tab\there"}) L("wildcard", s.Replace("\t","\\t"), s.WildCardToRegular().Replace("\t","\\t"));
    L("charname", CharacterNameExtensions.StringArrayToCharacterName(new[]{"cmd","Some","Name"}, 1), CharacterNameExtensions.StringArrayToCharacterName(new[]{"cmd","Name"}, 1), CharacterNameExtensions.StringArrayToCharacterName(new[]{"A","B","C","D"}, 0), CharacterNameExtensions.StringArrayToCharacterName(new[]{"A","B"}, 0));
    foreach (var v in new uint[]{0, 1, 3, 0x80000000, 0xFFFFFFFF, 6}) L("flags", v.ToString(), EnumHelper.NumFlags(v).ToString(), EnumHelper.HasMultiple(v).ToString());
    { ThreadSafeRandom.random = new Random(42); var list = new List<int>(); for (int i = 0; i < 10; i++) list.Add(i); list.Shuffle(); var sl = new List<string>(); foreach (var x in list) sl.Add(x.ToString()); L("shuffle", "42", string.Join(",", sl.ToArray())); }
    { ThreadSafeRandom.random = new Random(7); var a = new List<string>(); for (int i = 0; i < 5; i++) a.Add(ThreadSafeRandom.Next(1, 6).ToString()); for (int i = 0; i < 3; i++) a.Add(DB(ThreadSafeRandom.Next(0.25f, 0.75f))); for (int i = 0; i < 3; i++) a.Add(DB(ThreadSafeRandom.NextInterval(0.3f))); for (int i = 0; i < 3; i++) a.Add(DB(ThreadSafeRandom.NextIntervalMax(0.9f))); L("tsr", "7", string.Join(",", a.ToArray())); }
    L("product", FB(new List<float>{1.1f, 2.2f, 3.3f}.Product()), FB(new List<float>().Product()));
    foreach (var tk in new long[]{0, 10000000, 20000000, 900610000000, 1234567891234, 1728000000000, 36610000000, -10000000, 1840000000000 })
    { var ts = new TimeSpan(tk); L("tsfriendly", tk.ToString(), TSX.GetFriendlyString(ts), TSX.GetFriendlyLongString(ts), TSX.GetMonths(ts).ToString(), TSX.GetYears(ts).ToString()); }
    L("time", DB(Time.GetUnixTime(new DateTime(2024,1,5,0,7,9,45, DateTimeKind.Utc))), Time.GetDateTimeFromTimestamp(1700000000.123456789).Ticks.ToString(), Time.GetDateTimeFromTimestamp(-1.5).Ticks.ToString());
    L("common", new DateTime(2024,1,5,13,7,9).ToString("yyyy-MM-dd h:mm:ss tt"));
    Console.OutputEncoding = new UTF8Encoding(false);
    Console.Write(sb.ToString());
  }
}
