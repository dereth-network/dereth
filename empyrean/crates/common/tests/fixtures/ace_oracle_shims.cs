// Shims for ace_oracle_driver.cs. Adapted to C# 5 from ACE (ACEmulator), AGPL-3.0:
// Source/ACE.Common/ThreadSafeRandom.cs (seedable) and Source/ACE.Common/Extensions/TimeSpanExtensions.cs.
using System;
namespace ACE.Common {
  public static class ThreadSafeRandom {
    public static Random random = new Random(0);
    public static double Next(float min, float max) { return random.NextDouble() * (max - min) + min; }
    public static int Next(int min, int max) { return random.Next(min, max + 1); }
    public static double NextInterval(float qualityMod) { return Math.Max(0.0, random.NextDouble() - qualityMod); }
    private const double maxExclusive = 0.9999999999999999;
    public static double NextIntervalMax(float qualityMod) { return Math.Min(maxExclusive, random.NextDouble() + qualityMod); }
  }
  public static class TSX {
        public static string GetFriendlyString(TimeSpan timeSpan)
        {
            var numDays = timeSpan.ToString("%d");
            var numHours = timeSpan.ToString("%h");
            var numMinutes = timeSpan.ToString("%m");
            var numSeconds = timeSpan.ToString("%s");
            var sb = new System.Text.StringBuilder();
            if (numDays != "0") sb.Append(numDays + "d ");
            if (numHours != "0") sb.Append(numHours + "h ");
            if (numMinutes != "0") sb.Append(numMinutes + "m ");
            if (numSeconds != "0") sb.Append(numSeconds + "s ");
            return sb.ToString().Trim();
        }
        public static string GetFriendlyLongString(TimeSpan timeSpan)
        {
            var numDays = timeSpan.ToString("%d");
            var numHours = timeSpan.ToString("%h");
            var numMinutes = timeSpan.ToString("%m");
            var numSeconds = timeSpan.ToString("%s");
            var sb = new System.Text.StringBuilder();
            if (numDays != "0") sb.Append(numDays + " day" + ((timeSpan.Days > 1) ? "s" : "") + " ");
            if (numHours != "0") sb.Append(((numDays != "0") ? ", " : "") + numHours + " hour" + ((timeSpan.Hours > 1) ? "s" : "") + " ");
            if (numMinutes != "0") sb.Append(((numDays != "0" || numHours != "0") ? ", " : "") + numMinutes + " minute" + ((timeSpan.Minutes > 1) ? "s" : "") + " ");
            if (numSeconds != "0") sb.Append(((numDays != "0" || numHours != "0" || numMinutes != "0") ? "and " : "") + numSeconds + " second" + ((timeSpan.Seconds > 1) ? "s" : "") + " ");
            return sb.ToString().Trim();
        }
        public static uint SecondsPerMonth = 60 * 60 * 24 * 30;
        public static uint SecondsPerYear = 60 * 60 * 24 * 365;
        public static uint GetMonths(TimeSpan timeSpan) { return (uint)timeSpan.TotalSeconds / SecondsPerMonth; }
        public static uint GetYears(TimeSpan timeSpan) { return (uint)timeSpan.TotalSeconds / SecondsPerYear; }
  }
}
