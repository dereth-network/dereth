using System;
using System.Collections.Generic;

namespace Dereth.Decal
{
    /// <summary>
    /// The host's ops carried out: each as the Decal calls it names (<see cref="IRetail"/>), a line
    /// to hide (<see cref="ChatHides"/>), a view (<see cref="Views"/>), a mark
    /// (<see cref="Marks"/>), one of the game's keys (<see cref="Keys"/>) or a setting kept.
    /// </summary>
    /// <remarks>
    /// A salvage takes three frames, as the salvage panel does: the tool used (which opens the
    /// panel), then each item added, then the salvage.
    /// </remarks>
    public sealed class Ops
    {
        private readonly IRetail retail;
        private readonly ChatHides hides;
        private readonly Views views;
        private readonly Marks marks;
        private readonly Keys? keys;
        private readonly Action<string, string>? setting;
        private readonly Queue<Action> later = new Queue<Action>();

        /// <param name="keys">Where the game's keys go; none, and a key op does nothing.</param>
        /// <param name="setting">Where a setting the player changed is kept: its name and value.</param>
        public Ops(IRetail retail, ChatHides hides, Views views, Marks marks, Keys? keys = null, Action<string, string>? setting = null)
        {
            this.retail = retail;
            this.hides = hides;
            this.views = views;
            this.marks = marks;
            this.keys = keys;
            this.setting = setting;
        }

        /// <summary>The ops that failed in Decal, and the last failure.</summary>
        public int Failures { get; private set; }

        public string LastFailure { get; private set; } = "";

        /// <summary>The steps waiting for a later frame.</summary>
        public int Waiting => later.Count;

        /// <summary>The host's ops, at <paramref name="now"/>.</summary>
        public void Apply(IEnumerable<object?> ops, double now)
        {
            foreach (var o in ops)
            {
                if (!(o is Dictionary<string, object?> op))
                    continue;
                try
                {
                    One(op, now);
                }
                catch (Exception e)
                {
                    Failures++;
                    LastFailure = Json.Str(Json.Get(op, "op")) + ": " + e.Message;
                }
            }
        }

        /// <summary>One frame: the next step waiting, if any.</summary>
        public void Frame()
        {
            if (later.Count == 0)
                return;
            try
            {
                later.Dequeue()();
            }
            catch (Exception e)
            {
                Failures++;
                LastFailure = "a later step: " + e.Message;
            }
        }

        private void One(Dictionary<string, object?> op, double now)
        {
            int Id(string k) => Json.Id(Json.Get(op, k)) ?? 0;
            int Count(string k) => Json.Int(Json.Get(op, k)) ?? 0;
            bool Flag(string k) => Json.Bool(Json.Get(op, k));
            switch (Json.Str(Json.Get(op, "op")))
            {
                case "print": retail.AddChatText(Json.Str(Json.Get(op, "text")), Count("color")); break;
                case "hide": hides.Add(Json.Str(Json.Get(op, "text")), now); break;
                case "chat": retail.InvokeChatParser(Json.Str(Json.Get(op, "line"))); break;
                case "select": retail.SelectItem(Id("id")); break;
                case "use": retail.UseItem(Id("id")); break;
                case "apply": retail.ApplyItem(Id("item"), Id("target")); break;
                case "appraise": retail.RequestId(Id("id")); break;
                case "move": retail.MoveItem(Id("item"), Id("container"), Count("slot")); break;
                case "wield": retail.AutoWield(Id("item")); break;
                case "drop": retail.DropItem(Id("id")); break;
                case "give": retail.GiveItem(Id("item"), Id("target")); break;
                case "cast": retail.CastSpell(Id("spell"), Id("target")); break;
                case "stance": retail.SetCombatMode(Count("mode")); break;
                case "vendor_buy":
                    retail.VendorClearBuyList();
                    foreach (var i in Json.List(Json.Get(op, "items")))
                        retail.VendorAddBuyList(Json.Id(Json.Get(i, "id")) ?? 0, Json.Int(Json.Get(i, "count")) ?? 0);
                    retail.VendorBuyAll();
                    break;
                case "vendor_sell":
                    retail.VendorClearSellList();
                    foreach (var i in Json.List(Json.Get(op, "items")))
                        retail.VendorAddSellList(Json.Id(i) ?? 0);
                    retail.VendorSellAll();
                    break;
                case "trade_add": retail.TradeAdd(Id("item")); break;
                case "trade_accept": retail.TradeAccept(); break;
                case "trade_decline": retail.TradeDecline(); break;
                case "trade_reset": retail.TradeReset(); break;
                case "trade_end": retail.TradeEnd(); break;
                case "fellow_recruit": retail.FellowshipRecruit(Id("id")); break;
                case "fellow_dismiss": retail.FellowshipDismiss(Id("id")); break;
                case "fellow_leader": retail.FellowshipGrantLeader(Id("id")); break;
                case "fellow_open": retail.FellowshipSetOpen(Flag("open")); break;
                case "fellow_leave":
                    if (Flag("disband"))
                        retail.FellowshipDisband();
                    else
                        retail.FellowshipQuit();
                    break;
                case "salvage":
                    {
                        var items = new List<int>();
                        foreach (var i in Json.List(Json.Get(op, "items")))
                            items.Add(Json.Id(i) ?? 0);
                        retail.UseItem(Id("tool"));
                        later.Enqueue(() =>
                        {
                            foreach (var i in items)
                                retail.SalvagePanelAdd(i);
                        });
                        later.Enqueue(retail.SalvagePanelSalvage);
                        break;
                    }
                case "train_skill": retail.AddSkillExperience(Count("skill"), Id("xp")); break;
                case "train_attribute": retail.AddAttributeExperience(Count("attribute"), Id("xp")); break;
                case "train_vital": retail.AddVitalExperience(Count("vital"), Id("xp")); break;
                case "face": retail.FaceHeading(Json.Num(Json.Get(op, "heading")) ?? 0); break;
                case "autorun": retail.SetAutorun(Flag("on")); break;
                case "key": keys?.Press(Count("vk"), Count("scan"), Flag("extended"), Flag("down")); break;
                case "setting": setting?.Invoke(Json.Str(Json.Get(op, "name")), Json.Str(Json.Get(op, "value"))); break;
                case "logout": retail.Logout(); break;
                case "view":
                case "update":
                case "show":
                case "dispose":
                    views.Apply(op);
                    break;
                case "mark":
                case "unmark":
                    marks.Apply(op);
                    break;
            }
        }
    }
}
