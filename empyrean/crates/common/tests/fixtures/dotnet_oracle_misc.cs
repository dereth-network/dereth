// Oracle for tests/fixtures/dotnet_misc.tsv (empyrean-common, unit 0.3). Not ACE code.
// Build and run as dotnet_oracle.cs; the output is committed unchanged (.NET 8.0.22).
using System;
using System.Globalization;
using System.Collections.Generic;
using System.Text;
class P {
  static StringBuilder sb = new StringBuilder();
  static void L(params string[] f){ sb.Append(string.Join("\t", f)).Append('\n'); }
  static string DB(double d){ return "d:" + BitConverter.DoubleToInt64Bits(d).ToString("X16"); }
  static void Main() {
    System.Threading.Thread.CurrentThread.CurrentCulture = new CultureInfo("en-US");
    // Random (Net5CompatSeedImpl)
    foreach (var seed in new[]{0, 1, 42, -1, 12345, int.MinValue, int.MaxValue, 161803398, 100}) {
      var r = new Random(seed); var a = new List<string>();
      for (int i = 0; i < 8; i++) a.Add(r.Next().ToString()); L("rnd_next", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(DB(r.NextDouble())); L("rnd_double", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 12; i++) a.Add(r.Next(0, 11).ToString()); L("rnd_0_11", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(r.Next(int.MinValue, int.MaxValue).ToString()); L("rnd_large", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(r.Next(-5, 6).ToString()); L("rnd_m5_6", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(r.Next(100).ToString()); L("rnd_max100", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(DB(r.NextDouble() * (2.5f - 0.5f) + 0.5f)); L("rnd_float_0.5_2.5", seed.ToString(), string.Join(",", a.ToArray()));
      r = new Random(seed); a.Clear();
      for (int i = 0; i < 8; i++) a.Add(r.Next(7, 7).ToString()); L("rnd_7_7", seed.ToString(), string.Join(",", a.ToArray()));
    }
    // 10000th value for seed 0 after 9999 draws
    { var r = new Random(0); int v = 0; for (int i = 0; i < 10000; i++) v = r.Next(); L("rnd_next_10000", "0", v.ToString()); }
    // Dictionary order
    {
      var d = new Dictionary<int, string>(); var log = new List<string>();
      Action dump = () => { var ks = new List<string>(); foreach (var kv in d) ks.Add(kv.Key.ToString()); log.Add(string.Join(",", ks.ToArray())); };
      d[1]="a"; d[2]="b"; d[3]="c"; d[4]="d"; dump();
      d.Remove(2); dump(); d[5]="e"; dump();
      d.Remove(1); d.Remove(4); dump(); d[6]="f"; dump(); d[7]="g"; dump(); d[8]="h"; dump();
      d[3]="c2"; dump(); d.Remove(99); dump();
      d.Remove(5); d.Remove(6); d.Remove(7); d.Remove(8); d.Remove(3); dump(); d[9]="i"; d[10]="j"; dump();
      d.Clear(); d[11]="k"; d[12]="l"; dump();
      for (int i = 20; i < 40; i++) d[i] = "x"; for (int i = 20; i < 40; i += 3) d.Remove(i); dump(); for (int i = 100; i < 110; i++) d[i] = "y"; dump();
      d.Remove(11); d.Remove(100); d.Remove(12); d[200] = "z"; d[201] = "z"; d[202] = "z"; d[203] = "z"; dump();
      L("dict", string.Join("|", log.ToArray()));
      var h = new HashSet<uint>(); log.Clear();
      Action hdump = () => { var ks = new List<string>(); foreach (var k in h) ks.Add(k.ToString()); log.Add(string.Join(",", ks.ToArray())); };
      h.Add(10); h.Add(20); h.Add(30); h.Add(10); hdump(); h.Remove(20); h.Remove(10); hdump(); h.Add(40); hdump(); h.Add(50); h.Add(60); hdump(); h.Clear(); h.Add(1); hdump();
      L("hashset", string.Join("|", log.ToArray()));
    }
    // DateTime
    var dts = new[]{ new DateTime(2024, 1, 5, 0, 7, 9, 45), new DateTime(1999, 11, 2, 13, 30, 0), new DateTime(2017, 1, 31, 12, 0, 0), new DateTime(1, 1, 1), DateTime.MaxValue, new DateTime(2000, 2, 29, 23, 59, 59, 999), new DateTime(1970,1,1).AddTicks(1234567)};
    string[] dfs = {"yyyy-MM-dd HH:mm:ss", "MMM d yyyy h:mm tt", "yyyy-MM-dd hh:mm:ss,fff", "yyyy-MM-dd h:mm:ss tt", "yyyyMMddHHmmss", "ss", "mmtt", "", "G", "d", "MMMM dddd ddd dd d yy H m s f ff fffffff FFF", "%d", "%h", "yyyy/MM/dd"};
    foreach (var dt in dts) foreach (var f in dfs) L("dt", dt.Ticks.ToString(), f, dt.ToString(f));
    var epoch = new DateTime(1970, 1, 1, 0, 0, 0, 0, DateTimeKind.Utc);
    foreach (var s in new double[]{0, 1.5, 0.0000001, 0.00000015, -0.00000015, 1234567.891234, 1e9 + 0.1234567, -86400.5, 0.99999999, 1.23456789e-5, 1700000000.123456789})
    { var t = epoch.AddSeconds(s); L("addsec", DB(s), t.Ticks.ToString(), DB((t - epoch).TotalSeconds)); }
    foreach (var s in new double[]{-5, 2.5, 0.0001}) { var t = epoch.AddHours(s); L("addhours", DB(s), t.Ticks.ToString()); }
    foreach (var s in new double[]{1.5, 0.00000015, -2.75, 1e-8, 123.456789}) { var ts = TimeSpan.FromSeconds(s); L("fromsec", DB(s), ts.Ticks.ToString()); var tm = TimeSpan.FromMinutes(s); L("frommin", DB(s), tm.Ticks.ToString()); }
    foreach (var tk in new long[]{0, 1, 10000000, 36000000000, 864000000000, 900610000000, 1234567891234, -1234567891234, -10000000, 60000000000})
    { var ts = new TimeSpan(tk); L("ts", tk.ToString(), ts.ToString(), ts.ToString("%d"), ts.ToString("%h"), ts.ToString("%m"), ts.ToString("%s"), DB(ts.TotalSeconds), DB(ts.TotalMinutes), DB(ts.TotalHours), DB(ts.TotalDays), DB(ts.TotalMilliseconds), ts.Days + "," + ts.Hours + "," + ts.Minutes + "," + ts.Seconds + "," + ts.Milliseconds); }
    Console.OutputEncoding = new UTF8Encoding(false);
    Console.Write(sb.ToString());
  }
}
