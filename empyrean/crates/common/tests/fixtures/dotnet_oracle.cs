// Oracle for tests/fixtures/dotnet_format.tsv (empyrean-common, unit 0.3). Not ACE code.
//
// Build with the .NET Framework compiler (C# 5) and run on the .NET (Core) runtime:
//   csc.exe -nologo -out:o.dll -target:exe dotnet_oracle.cs
//   (o.runtimeconfig.json: {"runtimeOptions":{"tfm":"net8.0","framework":{"name":"Microsoft.NETCore.App","version":"8.0.0"}}})
//   dotnet o.dll > oracle.tsv
// The committed TSV was produced on .NET 8.0.22 and post-processed: F99 lines kept for the first
// 25 values only, and the `round` lines folded into one line per input (columns in the
// `#round-columns` header, results as double bit patterns).
using System;
using System.Globalization;
using System.Collections.Generic;
using System.Text;
class P {
  static StringBuilder sb = new StringBuilder();
  static void L(params string[] f){ sb.Append(string.Join("\t", f)).Append('\n'); }
  static string DB(double d){ return "d:" + BitConverter.DoubleToInt64Bits(d).ToString("X16"); }
  static string FB(float f){ return "f:" + BitConverter.ToInt32(BitConverter.GetBytes(f), 0).ToString("X8"); }
  static string Esc(string s){ return s.Replace("\\", "\\\\").Replace("\t", "\t").Replace("\n", "\n"); }
  static void Main() {
    System.Threading.Thread.CurrentThread.CurrentCulture = new CultureInfo("en-US");
    var rng = new Random(12345);
    var ds = new List<double>{0, -0.0, 0.5, 1.5, 2.5, -2.5, 3.5, 0.125, 0.375, 1.005, 2.675, 1234567.891, -1234.5, 0.001, 0.005, 0.015, 0.025, 1e14, 1e15, 1e16, 9e16, 1e17, 123456789012345678.0, 1e-4, 1e-5, 0.00012345, 0.0001234, 12345.6789, 0.05, -0.04, 999.9999, 999.5, 9999.5, 1e21, 0.1, 0.2, 0.3, 1.0/3, 2.0/3, 5e-324, 2.2250738585072014E-308, double.MaxValue, -double.MaxValue, double.NaN, double.PositiveInfinity, double.NegativeInfinity, 0.49999999999999994, 4503599627370497.0, 4503599627370496.5, 1e300, 123.456, -0.0049, -0.005, 99.95, 99.995, 0.95, 0.9999, 15.5, 16.5, 100, 1000000, 1e-10, 12.345e-7};
    for (int i = 0; i < 60; i++) ds.Add((rng.NextDouble() - 0.5) * Math.Pow(10, rng.Next(-8, 12)));
    for (int i = 0; i < 20; i++) ds.Add(Math.Round((rng.NextDouble()) * 1000, 3));
    string[] fs = {"","N0","N1","N2","N3","N4","F","F0","F1","F2","F3","F6","F99","P2","G4","R","0","000","00000","0.0","0.00","#.00","#,###0","####","#","#.#","0.##","#,##0.00"};
    foreach (var d in ds) foreach (var f in fs) L("fmt", DB(d), f, Esc(d.ToString(f)));
    var fl = new List<float>{0, -0.0f, 0.05f, 0.15f, 1.25f, -0.04f, 12.95f, 0.1f, 16777217f, 1e7f, 1e8f, 1e9f, 1e10f, 123456.7f, float.MaxValue, 1.0f/3, 2.5f, 0.45f, 40.25f, 33.35f, 2.675f, 1.115f, 1e-5f, 1e-4f, 0.0001234f, float.Epsilon, float.NaN, float.PositiveInfinity, 99.95f, 12.34567f};
    for (int i = 0; i < 40; i++) fl.Add((float)((rng.NextDouble() - 0.5) * Math.Pow(10, rng.Next(-5, 8))));
    string[] ffs = {"","N0","N1","N2","N4","F","F0","F2","F6","P2","G4","R","0","0.0","0.00","#.00","#,###0","####"};
    foreach (var d in fl) foreach (var f in ffs) L("fmt", FB(d), f, Esc(d.ToString(f)));
    long[] ls = {0, 1, -1, 7, 999, 1000, -1000, 1234567, 99999, 100000, long.MaxValue, long.MinValue, int.MinValue, int.MaxValue};
    string[] lfs = {"","N0","N2","X","X2","X4","X8","0","000","00000","####","#,###0","D","F","F2","P2","G4","0.0","#.00"};
    foreach (var l in ls) foreach (var f in lfs) L("fmt", "i64:" + l, f, Esc(l.ToString(f)));
    int[] isx = {-1, 255, int.MinValue, int.MaxValue, 0, 1000, 1234567, -1234567};
    foreach (var i in isx) foreach (var f in lfs) L("fmt", "i32:" + i, f, Esc(i.ToString(f)));
    uint[] us = {0, 1, 255, 4294967295u, 3000000000u};
    foreach (var i in us) foreach (var f in lfs) L("fmt", "u32:" + i, f, Esc(i.ToString(f)));
    ulong[] uls = {0, ulong.MaxValue, 12345678901234567890ul};
    foreach (var i in uls) foreach (var f in lfs) L("fmt", "u64:" + i, f, Esc(i.ToString(f)));
    short[] ss = {-1, 1000}; foreach (var i in ss) foreach (var f in new[]{"X","X4","N0"}) L("fmt", "i16:" + i, f, Esc(i.ToString(f)));
    ushort[] uss = {65535, 7}; foreach (var i in uss) foreach (var f in new[]{"X","X4","N0"}) L("fmt", "u16:" + i, f, Esc(i.ToString(f)));
    byte[] bs = {255, 7}; foreach (var i in bs) foreach (var f in new[]{"X","X2","X4","N0"}) L("fmt", "u8:" + i, f, Esc(i.ToString(f)));
    sbyte[] sbs = {-1, 7}; foreach (var i in sbs) foreach (var f in new[]{"X","X2","N0"}) L("fmt", "i8:" + i, f, Esc(i.ToString(f)));

    // Math.Round
    var rs = new List<double>{0, -0.0, 0.5, 1.5, 2.5, -0.5, -1.5, -2.5, 0.49999999999999994, -0.49999999999999994, 2.675, 1.005, 0.285, 1.115, 0.125, -0.125, 1e15 + 0.5, 1e16, 1e16 + 2, 4503599627370495.5, 4503599627370497, double.NaN, double.PositiveInfinity, double.NegativeInfinity, double.MaxValue, 5e-324, 123.456789, -123.456789, 0.045, 0.055, 1.45, 1.55, 2.345, 1234.5678, 99.995};
    for (int i = 0; i < 80; i++) rs.Add((rng.NextDouble() - 0.5) * Math.Pow(10, rng.Next(-4, 10)));
    for (int i = 0; i < 40; i++) rs.Add(rng.Next(-100000, 100000) / 1000.0 + 0.0005 * (rng.Next(0,2)*2-1));
    int[] modes = {0, 1, 2, 3, 4};
    foreach (var r in rs) {
      L("round", DB(r), "-", "-", DB(Math.Round(r)));
      foreach (var m in modes) L("round", DB(r), "-", m.ToString(), DB(Math.Round(r, (MidpointRounding)m)));
      foreach (var dg in new[]{0,1,2,3,4,15}) {
        L("round", DB(r), dg.ToString(), "-", DB(Math.Round(r, dg)));
        foreach (var m in modes) L("round", DB(r), dg.ToString(), m.ToString(), DB(Math.Round(r, dg, (MidpointRounding)m)));
      }
    }
    // Math.Max / Min
    var mm = new double[]{0, -0.0, 1, -1, double.NaN, double.PositiveInfinity, double.NegativeInfinity};
    foreach (var a in mm) foreach (var b in mm) { L("max", DB(a), DB(b), DB(Math.Max(a, b))); L("min", DB(a), DB(b), DB(Math.Min(a, b))); }
    var mf = new float[]{0, -0.0f, 1, float.NaN};
    foreach (var a in mf) foreach (var b in mf) { L("maxf", FB(a), FB(b), FB(Math.Max(a, b))); L("minf", FB(a), FB(b), FB(Math.Min(a, b))); }

    Console.OutputEncoding = new UTF8Encoding(false);
    Console.Write(sb.ToString());
  }
}
