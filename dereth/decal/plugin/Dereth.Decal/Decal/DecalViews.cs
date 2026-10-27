using System;
using System.Collections.Generic;
using System.Drawing;
using Decal.Adapter.Wrappers;

namespace Dereth.Decal.Plugin
{
    /// <summary>The plugins' windows as views of Decal's plugin.</summary>
    internal sealed class DecalViews : IViewBackend
    {
        private readonly PluginHost host;

        public DecalViews(PluginHost host) => this.host = host;

        public IView Load(string xml, IReadOnlyList<Dictionary<string, object?>> controls, ControlEvent events)
        {
            var view = new DecalView(host.LoadView(xml));
            foreach (var state in controls)
            {
                view.Apply(state);
                view.Wire(state, events);
            }
            return view;
        }
    }

    /// <summary>One Decal view, its controls set and read by their kind.</summary>
    internal sealed class DecalView : IView
    {
        private readonly ViewWrapper view;

        public DecalView(ViewWrapper view) => this.view = view;

        public bool Activated
        {
            get => view.Activated;
            set => view.Activated = value;
        }

        public Rectangle Position
        {
            get => view.Position;
            set => view.Position = value;
        }

        public void Dispose() => view.Dispose();

        private static string Str(Dictionary<string, object?> s, string k) => Json.Str(Json.Get(s, k));

        private static int Int(Dictionary<string, object?> s, string k, int otherwise) =>
            Json.Num(Json.Get(s, k)) is double d ? (int)Math.Round(Math.Max(int.MinValue, Math.Min(int.MaxValue, d))) : otherwise;

        public void Apply(Dictionary<string, object?> state)
        {
            var c = view.Controls[Str(state, "name")];
            switch (Str(state, "kind"))
            {
                case "text":
                    ((StaticWrapper)c).Text = Str(state, "text");
                    break;
                case "button":
                    ((PushButtonWrapper)c).Text = Str(state, "text");
                    break;
                case "toggle":
                    {
                        var box = (CheckBoxWrapper)c;
                        box.Text = Str(state, "text");
                        box.Checked = Json.Bool(Json.Get(state, "checked"));
                        break;
                    }
                case "meter":
                    {
                        var bar = (ProgressWrapper)c;
                        var text = Str(state, "text");
                        bar.MaxValue = 100;
                        bar.Value = Int(state, "value", 0);
                        bar.DrawText = text.Length > 0;
                        bar.PreText = text.Length > 0 ? text + " " : "";
                        break;
                    }
                case "list":
                    {
                        var list = (ListWrapper)c;
                        list.Clear();
                        foreach (var r in Json.List(Json.Get(state, "rows")))
                        {
                            var row = list.Add();
                            var cells = Json.List(r);
                            for (var i = 0; i < cells.Count && i < list.ColCount; i++)
                            {
                                var cell = cells[i];
                                if (cell is Dictionary<string, object?> styled)
                                {
                                    row[i][0] = Json.Str(Json.Get(styled, "text"));
                                    if (Json.Num(Json.Get(styled, "color")) is double rgb)
                                        row[i].Color = Color.FromArgb(unchecked((int)(0xFF000000u | ((uint)rgb & 0xFFFFFF))));
                                }
                                else
                                {
                                    row[i][0] = Json.Str(cell);
                                }
                            }
                        }
                        break;
                    }
                case "choice":
                    {
                        var choice = (ChoiceWrapper)c;
                        choice.Clear();
                        foreach (var o in Json.List(Json.Get(state, "options")))
                            choice.Add(Json.Str(o), null);
                        var selected = Int(state, "selected", -1);
                        if (selected >= 0 && selected < choice.Count)
                            choice.Selected = selected;
                        break;
                    }
                case "input":
                    {
                        // Set only when it differs, so text the player is typing keeps its caret.
                        var box = (TextBoxWrapper)c;
                        var text = Str(state, "text");
                        if (box.Text != text)
                            box.Text = text;
                        break;
                    }
                case "slider":
                    {
                        var slider = (SliderWrapper)c;
                        slider.Minimum = Int(state, "min", 0);
                        slider.Maximum = Int(state, "max", 100);
                        slider.Position = Int(state, "value", 0);
                        break;
                    }
                case "tabs":
                    ((NotebookWrapper)c).ActiveTab = Int(state, "page", 0);
                    break;
            }
        }

        /// <summary>A control that stands for a node: its events, as the host reads them.</summary>
        public void Wire(Dictionary<string, object?> state, ControlEvent events)
        {
            if (Json.Get(state, "node") == null)
                return;
            var name = Str(state, "name");
            var c = view.Controls[name];
            switch (Str(state, "kind"))
            {
                case "button":
                    ((PushButtonWrapper)c).Hit += (s, e) => events(name, "click", null);
                    break;
                case "toggle":
                    ((CheckBoxWrapper)c).Change += (s, e) => events(name, "change", e.Checked);
                    break;
                case "input":
                    ((TextBoxWrapper)c).Change += (s, e) => events(name, "change", e.Text);
                    break;
                case "choice":
                    ((ChoiceWrapper)c).Change += (s, e) => events(name, "change", e.Index);
                    break;
                case "slider":
                    ((SliderWrapper)c).Change += (s, e) => events(name, "change", e.Index);
                    break;
                case "list":
                    ((ListWrapper)c).Selected += (s, e) => events(name, "select", e.Row);
                    break;
                case "tabs":
                    ((NotebookWrapper)c).Change += (s, e) => events(name, "change", e.Index);
                    break;
            }
        }
    }
}
