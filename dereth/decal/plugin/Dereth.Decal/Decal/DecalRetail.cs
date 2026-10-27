using Decal.Adapter.Wrappers;

namespace Dereth.Decal.Plugin
{
    /// <summary>The host's calls as Decal's own: each makes the client do or send what its interface would.</summary>
    internal sealed class DecalRetail : IRetail
    {
        private readonly HooksWrapper a;

        public DecalRetail(HooksWrapper actions) => a = actions;

        public void AddChatText(string text, int color) => a.AddChatText(text, color);
        public void InvokeChatParser(string line) => a.InvokeChatParser(line);
        public void SelectItem(int id) => a.SelectItem(id);
        public void UseItem(int id) => a.UseItem(id, 0);
        public void ApplyItem(int item, int target) => a.ApplyItem(item, target);
        public void RequestId(int id) => a.RequestId(id);
        public void MoveItem(int item, int container, int slot) => a.MoveItem(item, container, slot, false);
        public void AutoWield(int item) => a.AutoWield(item);
        public void DropItem(int id) => a.DropItem(id);
        public void GiveItem(int item, int target) => a.GiveItem(item, target);
        public void CastSpell(int spell, int target) => a.CastSpell(spell, target);
        public void SetCombatMode(int mode) => a.SetCombatMode((CombatState)mode);
        public void VendorClearBuyList() => a.VendorClearBuyList();
        public void VendorAddBuyList(int id, int count) => a.VendorAddBuyList(id, count);
        public void VendorBuyAll() => a.VendorBuyAll();
        public void VendorClearSellList() => a.VendorClearSellList();
        public void VendorAddSellList(int id) => a.VendorAddSellList(id);
        public void VendorSellAll() => a.VendorSellAll();
        public void TradeAdd(int id) => a.TradeAdd(id);
        public void TradeAccept() => a.TradeAccept();
        public void TradeDecline() => a.TradeDecline();
        public void TradeReset() => a.TradeReset();
        public void TradeEnd() => a.TradeEnd();
        public void FellowshipRecruit(int id) => a.FellowshipRecruit(id);
        public void FellowshipDismiss(int id) => a.FellowshipDismiss(id);
        public void FellowshipGrantLeader(int id) => a.FellowshipGrantLeader(id);
        public void FellowshipSetOpen(bool open) => a.FellowshipSetOpen(open);
        public void FellowshipQuit() => a.FellowshipQuit();
        public void FellowshipDisband() => a.FellowshipDisband();
        public void SalvagePanelAdd(int id) => a.SalvagePanelAdd(id);
        public void SalvagePanelSalvage() => a.SalvagePanelSalvage();

        // Decal's skill, attribute and vital numbers for these calls are the game's own ids.
        public void AddSkillExperience(int skill, int xp) => a.AddSkillExperience((SkillType)skill, xp);
        public void AddAttributeExperience(int attribute, int xp) => a.AddAttributeExperience((AttributeType)attribute, xp);
        public void AddVitalExperience(int vital, int xp) => a.AddVitalExperience((VitalType)vital, xp);

        public void FaceHeading(double heading) => a.FaceHeading(heading, true);
        public void SetAutorun(bool on) => a.SetAutorun(on);
        public void Logout() => a.Logout();
    }
}
