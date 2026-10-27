namespace Dereth.Decal
{
    /// <summary>
    /// The calls into the retail client that the host's ops become: each is one of Decal's own,
    /// which makes the client do or send what its own interface would.
    /// </summary>
    public interface IRetail
    {
        /// <summary>A line in the chat window, in the color of chat type <paramref name="color"/>.</summary>
        void AddChatText(string text, int color);

        /// <summary>A line run through the client's chat parser, as if typed.</summary>
        void InvokeChatParser(string line);

        void SelectItem(int id);
        void UseItem(int id);
        void ApplyItem(int item, int target);
        void RequestId(int id);
        void MoveItem(int item, int container, int slot);
        void AutoWield(int item);
        void DropItem(int id);
        void GiveItem(int item, int target);
        void CastSpell(int spell, int target);

        /// <summary>The stance: 1 peace, 2 melee, 4 missile, 8 magic.</summary>
        void SetCombatMode(int mode);

        void VendorClearBuyList();
        void VendorAddBuyList(int id, int count);
        void VendorBuyAll();
        void VendorClearSellList();
        void VendorAddSellList(int id);
        void VendorSellAll();

        void TradeAdd(int id);
        void TradeAccept();
        void TradeDecline();
        void TradeReset();
        void TradeEnd();

        void FellowshipRecruit(int id);
        void FellowshipDismiss(int id);
        void FellowshipGrantLeader(int id);
        void FellowshipSetOpen(bool open);
        void FellowshipQuit();
        void FellowshipDisband();

        void SalvagePanelAdd(int id);
        void SalvagePanelSalvage();

        void AddSkillExperience(int skill, int xp);
        void AddAttributeExperience(int attribute, int xp);
        void AddVitalExperience(int vital, int xp);

        /// <summary>Turn to a heading, in degrees clockwise from north.</summary>
        void FaceHeading(double heading);

        void SetAutorun(bool on);
        void Logout();
    }
}
